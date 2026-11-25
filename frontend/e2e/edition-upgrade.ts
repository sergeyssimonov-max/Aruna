import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync } from 'node:fs'
import { basename, join, relative, isAbsolute } from 'node:path'
import type { CorpusLocation } from '../src/bindings'

function tree(root: string) {
  const files = new Map<string, string>()
  function visit(path: string) {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const child = join(path, entry.name)
      if (entry.isDirectory()) visit(child)
      else if (entry.isFile())
        files.set(
          relative(root, child),
          createHash('sha256').update(readFileSync(child)).digest('hex'),
        )
      else throw new Error('Unexpected non-file in the library')
    }
  }
  visit(root)
  return files
}

// Opt in with a real, previously published Beta 0.3 package. The runner copies
// it to an isolated home. This test exercises the live collection, including
// editions published after Beta 0.4; its expected counts come from the new package.
export function editionUpgrade(): void {
  describe('updating an installed edition', () => {
    it('automatically rebuilds the library and keeps every file of Beta 0.3 readable', async () => {
      const old = process.env.ARUNA_E2E_PREVIOUS_LIBRARY
      const root = process.env.ARUNA_E2E_ROOT
      if (!old || !root)
        throw new Error('The upgrade scenario needs an isolated home and a Beta 0.3 package')
      await browser.waitUntil(
        async () => {
          const page = (await $('main').getText()).replace(/\s+/g, ' ')
          if (page.includes('Можно открыть прежнюю опись') || page.includes('Собрать не удалось'))
            throw new Error(page)
          return (await $('h1').getText()).endsWith('собрана')
        },
        {
          timeout: 540_000,
          interval: 500,
          timeoutMsg: 'The library did not update on opening the application',
        },
      )
      const location = (await browser.tauri.execute(({ core }) =>
        core.invoke('corpus_location'),
      )) as CorpusLocation
      const within = relative(root, location.package)
      expect(within && !within.startsWith('..') && !isAbsolute(within)).toBe(true)
      const manifest = JSON.parse(readFileSync(join(location.package, 'manifest.json'), 'utf8'))
      expect(manifest.source.zenodo.index).toBeGreaterThan(1)
      expect(manifest.source.archive_md5).toBe(manifest.source.zenodo.md5)
      expect(manifest.pdfs.built + manifest.pdfs.refused).toBe(manifest.counts.documents)
      const inventory = readFileSync(location.inventory, 'utf8')
      expect(manifest.package).not.toBe('TLHdig')
      if (manifest.source.zenodo.record === 24373727)
        expect(manifest.package).toBe('TLHdig_Beta_0.4')
      expect(basename(location.package)).toBe(manifest.package)
      expect(basename(location.inventory)).toBe(manifest.package + '.html')
      expect(manifest.inventory).toBe(basename(location.inventory))
      expect(inventory).toContain(manifest.source.zenodo.date)
      expect(inventory).not.toContain('DOI 10.5281/zenodo.')
      expect(inventory).not.toContain('CC BY 4.0')
      const summary = (await $('main').getText()).replace(/\s+/g, ' ')
      expect(summary).toContain(manifest.source.zenodo.date)
      expect(summary).toContain('Открыть опись')
      const original = tree(old)
      const preserved = tree(join(location.downloads, 'TLHdig_Beta_0.3'))
      assert.equal(preserved.size, original.size, 'File count in the original library')
      for (const [path, hash] of original) assert.equal(preserved.get(path), hash, path)
      const fits = await browser.execute(() => {
        const content = document.querySelector('.ready')?.getBoundingClientRect()
        const footer = document.querySelector('.foot')?.getBoundingClientRect()
        return (
          !!content &&
          !!footer &&
          content.top >= 0 &&
          content.bottom <= footer.top + 1 &&
          [...document.querySelectorAll('button')].every(
            (button) => button.getBoundingClientRect().bottom <= innerHeight,
          )
        )
      })
      expect(fits).toBe(true)
      console.info(
        'Published edition',
        manifest.source.zenodo.record,
        manifest.counts,
        manifest.pdfs,
        preserved.size,
        'legacy files verified',
      )
    }).timeout(600_000)
  })
}
