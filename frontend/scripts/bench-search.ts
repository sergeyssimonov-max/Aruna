/**
 * How long the inventory's search takes, in the browsers a reader opens it in.
 *
 * ```text
 * node scripts/bench-search.ts <package-dir> [chrome|webkit|both]
 * ```
 *
 * `<package-dir>` is a `TLHdig_Beta_0.3` folder built for the purpose —
 * `export_beta <archive.zip> <sandbox>` — and never the reader's own: a
 * `bench-search.html` is written beside the inventory for the length of the
 * run, so that its links and `@font-face` resolve exactly as the inventory's
 * do. A folder under `~/Downloads` is refused for that reason.
 *
 * The search is `src/inventory/filter.ts`, bundled into the page. It runs in
 * whatever browser opens `TLHdig_Beta_0.3.html` and never in the desktop
 * window, so that is where it is measured:
 *
 * - `chrome` — the installed Chrome as a separate headless process with a
 *   throwaway profile, so a Chrome already open on the machine is not touched;
 * - `webkit` — the system WKWebView through `bench-search.swift`, which is the
 *   engine Safari runs on this machine. No window is shown.
 *
 * The page is driven the way typing drives it: the query is put in the box and
 * an `input` event dispatched, which is all `filter.ts` listens to. Each
 * repetition starts from the full table — the box is emptied first, and the
 * emptying is reported as its own row. Nothing here is a keystroke.
 */

import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const INVENTORY = 'TLHdig_Beta_0.3.html'
const BENCH_PAGE = 'bench-search.html'
const CHROME =
  process.env.ARUNA_CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
const DEADLINE_MS = 30 * 60 * 1000

/**
 * Runs inside the page, after the inventory's own script has attached the
 * filter. Serialised with `toString()`, so it may use nothing from this module.
 */
function harness(): void {
  // The five queries the retired site was measured with, and the two its
  // commit added (`daf6d14`).
  const QUERIES = ['k', 'kbo', 'hit', 'schwemer', '2021', 'cth 547', 'zzzznope']
  const REPS = 25
  const WARMUP = 3

  type Sink = { postMessage(body: string): void }
  const bridge = window as unknown as { webkit?: { messageHandlers?: { bench?: Sink } } }
  const send = (body: unknown): void => {
    const text = JSON.stringify(body)
    const sink = bridge.webkit?.messageHandlers?.bench
    if (sink) sink.postMessage(text)
    else console.log('BENCH ' + text)
  }

  // The filter attaches synchronously in the script before this one, so the
  // page is searchable from here on.
  const readyAt = performance.now()
  const input = document.getElementById('q') as HTMLInputElement | null
  const hint = document.getElementById('hint')
  if (!input) {
    send({ done: true, error: 'no search box in the page' })
    return
  }

  // A frame is waited for with a deadline: a view that is not on screen may
  // never draw, and then only the synchronous figures are reported.
  let frames = true
  const frame = (): Promise<number | null> =>
    new Promise((settle) => {
      let settled = false
      const guard = setTimeout(() => {
        if (settled) return
        settled = true
        frames = false
        settle(null)
      }, 1000)
      requestAnimationFrame(() =>
        setTimeout(() => {
          if (settled) return
          settled = true
          clearTimeout(guard)
          settle(performance.now())
        }, 0),
      )
    })

  type Sample = { handler: number; layout: number; frame: number | null }
  const once = async (query: string): Promise<Sample> => {
    const t0 = performance.now()
    input.value = query
    input.dispatchEvent(new Event('input'))
    const t1 = performance.now()
    void document.body.offsetHeight
    const t2 = performance.now()
    const drawn = frames ? await frame() : null
    return { handler: t1 - t0, layout: t2 - t0, frame: drawn === null ? null : drawn - t0 }
  }

  const stats = (xs: number[]) => {
    const s = [...xs].sort((a, b) => a - b)
    const at = (p: number) => s[Math.min(s.length - 1, Math.round(p * (s.length - 1)))]
    return { min: s[0], p50: at(0.5), p95: at(0.95), max: s[s.length - 1] }
  }
  const summary = (samples: Sample[]) => {
    const drawn = samples.map((x) => x.frame).filter((x): x is number => x !== null)
    return {
      handler: stats(samples.map((x) => x.handler)),
      layout: stats(samples.map((x) => x.layout)),
      frame: drawn.length === samples.length ? stats(drawn) : null,
    }
  }

  // The finest step the clock takes here: browsers coarsen it, and a figure
  // below it says nothing.
  let resolution = Infinity
  for (let i = 0; i < 5; i++) {
    const a = performance.now()
    let b = a
    while (b === a) b = performance.now()
    resolution = Math.min(resolution, b - a)
  }

  const run = async (): Promise<void> => {
    if (document.readyState !== 'complete') {
      await new Promise((loaded) => addEventListener('load', loaded, { once: true }))
    }
    const nav = performance.getEntriesByType('navigation')[0] as
      PerformanceNavigationTiming | undefined
    const clears: Sample[] = []
    const results = []
    for (const query of QUERIES) {
      for (let i = 0; i < WARMUP; i++) {
        await once('')
        await once(query)
      }
      const samples: Sample[] = []
      for (let i = 0; i < REPS; i++) {
        clears.push(await once(''))
        samples.push(await once(query))
      }
      results.push({
        query,
        hint: hint ? hint.textContent : null,
        groups: document.querySelectorAll('#inv tbody tr.group:not([hidden])').length,
        rows: document.querySelectorAll('#inv tbody tr:not(.group):not([hidden])').length,
        ...summary(samples),
      })
      send({ done: false, progress: query, at: performance.now() })
    }
    send({
      done: true,
      userAgent: navigator.userAgent,
      resolution,
      readyAt,
      domContentLoaded: nav ? nav.domContentLoadedEventEnd : null,
      load: nav ? nav.loadEventEnd : null,
      frames,
      reps: REPS,
      results,
      clear: summary(clears),
    })
  }
  void run()
}

type Report = {
  done: true
  error?: string
  userAgent: string
  resolution: number
  readyAt: number
  domContentLoaded: number | null
  load: number | null
  frames: boolean
  reps: number
  results: Array<{
    query: string
    hint: string | null
    groups: number
    rows: number
    handler: Stats
    layout: Stats
    frame: Stats | null
  }>
  clear: { handler: Stats; layout: Stats; frame: Stats | null }
}
type Stats = { min: number; p50: number; p95: number; max: number }
type Progress = { done: false; progress: string; at: number }

/**
 * A message from the page: the report ends the run, a progress line only says
 * how far it has come. WebKit takes several times longer over this page than
 * Chrome, and a run that is slow should not look like one that is stuck.
 */
function settle(
  message: Report | Progress,
  deadline: ReturnType<typeof setTimeout>,
  done: (report: Report) => void,
): void {
  if (!message.done) {
    console.error(`  ${message.progress} done at ${(message.at / 1000).toFixed(0)} s`)
    return
  }
  clearTimeout(deadline)
  done(message)
}

/** Lines of a child's stream, one at a time. */
function lines(stream: NodeJS.ReadableStream, each: (line: string) => void): void {
  let buffer = ''
  stream.setEncoding('utf8')
  stream.on('data', (chunk: string) => {
    buffer += chunk
    let at: number
    while ((at = buffer.indexOf('\n')) >= 0) {
      each(buffer.slice(0, at))
      buffer = buffer.slice(at + 1)
    }
  })
}

function runChrome(page: string): Promise<Report> {
  const profile = mkdtempSync(join(tmpdir(), 'aruna-bench-chrome-'))
  const child = spawn(CHROME, [
    '--headless=new',
    `--user-data-dir=${profile}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-extensions',
    '--window-size=1280,900',
    '--enable-logging=stderr',
    '--v=0',
    pathToFileURL(page).href,
  ])
  return new Promise<Report>((done, fail) => {
    const deadline = setTimeout(() => fail(new Error('chrome: no result in time')), DEADLINE_MS)
    lines(child.stderr, (line) => {
      // [..:INFO:CONSOLE(n)] "BENCH {...}", source: file:///... (n)
      const from = line.indexOf('"BENCH ')
      const to = line.lastIndexOf('", source:')
      if (from < 0 || to < from) return
      settle(JSON.parse(line.slice(from + 7, to)) as Report | Progress, deadline, done)
    })
    child.on('exit', (code) => fail(new Error(`chrome exited with ${code} before a result`)))
  }).finally(() => {
    child.kill()
    rmSync(profile, { recursive: true, force: true })
  })
}

function runWebKit(page: string): Promise<Report> {
  const runner = join(dirname(fileURLToPath(import.meta.url)), 'bench-search.swift')
  const child = spawn('swift', [runner, page], { stdio: ['ignore', 'pipe', 'inherit'] })
  return new Promise<Report>((done, fail) => {
    const deadline = setTimeout(() => fail(new Error('webkit: no result in time')), DEADLINE_MS)
    lines(child.stdout, (line) => {
      if (!line.startsWith('{')) return
      settle(JSON.parse(line) as Report | Progress, deadline, done)
    })
    child.on('exit', (code) => fail(new Error(`webkit runner exited with ${code} before a result`)))
  }).finally(() => child.kill())
}

const ms = (x: number) => x.toFixed(1).padStart(7)

function print(engine: string, report: Report): void {
  if (report.error) {
    console.log(`${engine}: ${report.error}`)
    return
  }
  console.log(`\n${engine}: ${report.userAgent}`)
  console.log(
    `clock step ${report.resolution.toFixed(3)} ms; searchable at ${report.readyAt.toFixed(0)} ms,` +
      ` load at ${report.load?.toFixed(0) ?? '?'} ms; frames ${report.frames ? 'drawn' : 'not drawn'}`,
  )
  console.log(`median / p95 over ${report.reps} runs, ms\n`)
  console.log('query        groups   rows  handler     p95   +layout     p95  +frame     p95  hint')
  const row = (
    name: string,
    groups: string,
    rows: string,
    x: { handler: Stats; layout: Stats; frame: Stats | null },
    hint: string,
  ) =>
    console.log(
      name.padEnd(11) +
        groups.padStart(7) +
        rows.padStart(7) +
        ms(x.handler.p50) +
        ms(x.handler.p95) +
        ms(x.layout.p50) +
        ms(x.layout.p95) +
        (x.frame ? ms(x.frame.p50) + ms(x.frame.p95) : '      –       –') +
        '  ' +
        hint,
    )
  for (const r of report.results) {
    row(r.query, String(r.groups), String(r.rows), r, r.hint ?? '')
  }
  row('(empty)', '', '', report.clear, 'the full table back')
}

async function main(): Promise<number> {
  const [dirArg, engineArg = 'both'] = process.argv.slice(2)
  if (!dirArg || !['chrome', 'webkit', 'both'].includes(engineArg)) {
    console.error('usage: node scripts/bench-search.ts <package-dir> [chrome|webkit|both]')
    return 2
  }
  const dir = resolve(dirArg)
  if ((dir + sep).startsWith(join(homedir(), 'Downloads') + sep)) {
    console.error(`${dir} is under ~/Downloads: build a package for this elsewhere`)
    return 2
  }
  const inventory = join(dir, INVENTORY)
  if (!existsSync(inventory)) {
    console.error(`no ${INVENTORY} in ${dir}`)
    return 2
  }

  const html = readFileSync(inventory, 'utf8')
  const end = html.lastIndexOf('</body>')
  if (end < 0) {
    console.error(`${INVENTORY} has no </body>`)
    return 2
  }
  const page = join(dir, BENCH_PAGE)
  writeFileSync(
    page,
    html.slice(0, end) + `<script>(${harness.toString()})()</script>\n` + html.slice(end),
  )
  try {
    if (engineArg !== 'webkit') print('chrome', await runChrome(page))
    if (engineArg !== 'chrome') print('webkit', await runWebKit(page))
  } finally {
    unlinkSync(page)
  }
  return 0
}

process.exitCode = await main()
