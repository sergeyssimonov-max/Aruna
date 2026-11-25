import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { isAbsolute, join, relative } from 'node:path'
import type { BuildFailure, BuildProgress, CorpusLocation, CorpusStats } from '../src/bindings'

declare global {
  interface Window {
    arunaBuildProbe?: { events: BuildProgress[]; unlisten: () => void }
  }
}

/** Compare every file, including paths, without retaining the corpus in RAM. */
export function filesBelow(root: string): [string, string][] {
  const files: [string, string][] = []
  const visit = (folder: string) => {
    for (const entry of readdirSync(folder, { withFileTypes: true }).sort((a, b) =>
      a.name.localeCompare(b.name),
    )) {
      const path = join(folder, entry.name)
      if (entry.isDirectory()) visit(path)
      else {
        if (!entry.isFile()) throw new Error(`Unexpected non-file in E2E output: ${path}`)
        files.push([
          relative(root, path),
          createHash('sha256').update(readFileSync(path)).digest('hex'),
        ])
      }
    }
  }
  visit(root)
  return files
}

async function heard(): Promise<BuildProgress[]> {
  return await browser.tauri.execute(() => window.arunaBuildProbe?.events ?? [])
}

export function corpusLifecycle(): void {
  describe('Aruna native build lifecycle with an isolated corpus cache', () => {
    after(async () => {
      await browser.tauri.execute(async ({ core }) => {
        window.arunaBuildProbe?.unlisten()
        delete window.arunaBuildProbe
        await core.invoke('cancel_build')
      })
    })

    it('publishes a build, refuses a concurrent one and preserves every file after cancellation', async function () {
      // This is a whole-corpus build with PDFs, not part of the 60-second smoke
      // suite. Its own deadline leaves time for a second run cancelled in PDF.
      const root = process.env.ARUNA_E2E_ROOT
      if (!root) throw new Error('The corpus scenario requires the WDIO isolation directory')
      const location = (await browser.tauri.execute(({ core }) =>
        core.invoke('corpus_location'),
      )) as CorpusLocation
      const within = relative(root, location.downloads)
      if (!within || within.startsWith('..') || isAbsolute(within)) {
        throw new Error('The application did not select an isolated downloads directory')
      }
      mkdirSync(location.downloads, { recursive: true })
      writeFileSync(join(location.downloads, 'reader-note.txt'), 'Keep this file unchanged.\n')

      await browser.tauri.execute(async () => {
        const api = window as typeof window & {
          __TAURI__: { event: typeof import('@tauri-apps/api/event') }
        }
        const probe = { events: [] as BuildProgress[], unlisten: () => {} }
        probe.unlisten = await api.__TAURI__.event.listen<BuildProgress>(
          'build-progress',
          ({ payload }) => {
            probe.events.push(payload)
          },
        )
        window.arunaBuildProbe = probe
      })

      await $('[data-testid="primary"]').click()
      await browser.waitUntil(
        async () => (await heard()).some(({ stage }) => stage === 'archive-from-cache'),
        { timeout: 30_000 },
      )
      const first = (await heard()).slice(0, 2)
      expect(first.map(({ stage }) => stage)).toEqual(['zenodo-notice', 'archive-from-cache'])
      expect(first[0]?.note).toBe(
        'Связи с Zenodo нет. Для сборки используется сохраненное издание Beta 0.3 (2026-05-21).',
      )
      console.info('Corpus lifecycle: native build is reading the isolated cache')
      const busy = await browser.tauri.execute(({ core }) =>
        core.invoke('build_corpus', { destination: null }).then(
          () => null,
          (failure: unknown) => failure as BuildFailure,
        ),
      )
      expect(busy).toEqual({
        code: 'busy',
        phase: null,
        message: 'сборка уже идет',
        retryable: true,
        cancelled: false,
      })

      await browser.waitUntil(
        async () => {
          const heading = await $('h1').getText()
          if (heading === 'Собрать не удалось') throw new Error(await $('main').getText())
          return heading.endsWith('собрана')
        },
        { timeout: 300_000, interval: 500, timeoutMsg: 'The native build did not finish' },
      )
      const summary = (await $('main').getText()).replace(/\s+/g, ' ')
      for (const text of [
        'Документов – 23 936',
        'Групп CTH – 663',
        'PDF – 23 713',
        'Самая большая группа – CTH 832 (4 480)',
        'Групп из одной рукописи – 116',
        'Не могут быть преобразованы в PDF – 223 из 23 936',
        'В описи ссылка на такой документ открывает его XML.',
      ])
        expect(summary).toContain(text)
      expect(summary).not.toContain('Без PDF')
      const fits = await browser.execute(() => {
        const buttons = Array.from(document.querySelectorAll('button'))
        const content = document.querySelector('.ready')?.getBoundingClientRect()
        const footer = document.querySelector('.foot')?.getBoundingClientRect()
        return (
          !!content &&
          !!footer &&
          content.top >= 0 &&
          content.bottom <= footer.top + 1 &&
          buttons.length === 3 &&
          buttons.every((button) => {
            const box = button.getBoundingClientRect()
            return (
              box.top >= 0 && box.bottom <= innerHeight && box.left >= 0 && box.right <= innerWidth
            )
          })
        )
      })
      expect(fits).toBe(true)
      const completed = await heard()
      expect(new Set(completed.map(({ job }) => job)).size).toBe(1)
      expect(completed.some(({ stage }) => stage === 'downloading')).toBe(false)
      const pdf = completed.filter(({ stage }) => stage === 'writing-pdfs')
      expect(pdf[0]?.built).toBe(0)
      expect(pdf.at(-1)?.done).toBe(pdf.at(-1)?.total)
      expect(pdf.at(-1)?.built).toBe(23713)
      const checked = completed.findIndex(({ stage }) => stage === 'checking-package')
      const published = completed.findIndex(({ stage }) => stage === 'checking-published')
      expect(checked).toBeGreaterThan(-1)
      expect(published).toBeGreaterThan(checked)
      expect(completed[published]?.built).toBe(23713)
      const stats = (await browser.tauri.execute(
        ({ core }, path) => core.invoke('corpus_stats', { path }),
        location.package,
      )) as CorpusStats
      expect(stats.manuscripts).toBe(23936)
      expect(stats.groups).toBe(663)
      expect(readFileSync(location.inventory, 'utf8')).toContain('.pdf')
      const beforeCancel = filesBelow(location.downloads)
      expect(beforeCancel.filter(([file]) => file.endsWith('.xml'))).toHaveLength(23936)
      expect(beforeCancel.filter(([file]) => file.endsWith('.pdf'))).toHaveLength(23713)
      // Optional evidence for comparing two revisions on the same corpus.
      // Refuse to overwrite a previous run's evidence.
      const outputManifest = process.env.ARUNA_E2E_OUTPUT_MANIFEST
      if (outputManifest) {
        writeFileSync(outputManifest, `${JSON.stringify(beforeCancel, null, 2)}\n`, { flag: 'wx' })
      }
      console.info('Corpus lifecycle: 23936 XML and 23713 PDF files published')

      await browser.tauri.execute(() => {
        if (window.arunaBuildProbe) window.arunaBuildProbe.events.length = 0
      })
      await $('button=Пересобрать').click()
      await browser.waitUntil(
        async () =>
          (await heard()).some(({ stage, built }) => stage === 'writing-pdfs' && (built ?? 0) > 0),
        { timeout: 60_000, timeoutMsg: 'The next build did not reach the PDF phase' },
      )
      await $('[data-testid="primary"]').click()
      await browser.waitUntil(async () => (await $('h1').getText()) === 'Остановлено', {
        timeout: 30_000,
        timeoutMsg: 'Cancellation did not reach the interface',
      })
      const cancelled = await heard()
      expect(new Set(cancelled.map(({ job }) => job)).size).toBe(1)
      expect(cancelled[0]?.job).not.toBe(completed[0]?.job)
      expect(cancelled.some(({ stage }) => stage === 'checking-published')).toBe(false)
      expect(filesBelow(location.downloads)).toEqual(beforeCancel)
      await expect($('[data-testid="primary"]')).toBeEnabled()
      console.info('Corpus lifecycle: cancellation preserved every output path and SHA-256')
    }).timeout(600_000)
  })
}
