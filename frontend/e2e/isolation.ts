import { execFileSync } from 'node:child_process'
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from 'node:fs'
import { createServer } from 'node:net'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'

/**
 * The names Aruna's files go under in a `Library`: the bundle identifier, the
 * one before it, and the two binary names an unbundled run is keyed by
 * (`WebKit/aruna-desktop`, and `WebKit/aruna` from before the shell had a name
 * of its own). A suffix after a dot is the same owner: `.plist`, `.savedState`.
 */
const OURS = /^(com\.sergeyssimonov\.aruna|ru\.hippo\.aruna|aruna-desktop|aruna)(\..*)?$/i

/**
 * Every file and folder of Aruna's in `library`, with its size and the time it
 * last changed: the folders two levels down whose name is Aruna's
 * (`WebKit/aruna-desktop`, `Application Support/com.sergeyssimonov.aruna`, …)
 * and everything under them.
 *
 * Two levels, because that is where macOS puts what an application keeps; a
 * folder the system will not let this process read (`Mail`, `Safari` without
 * full disk access) is passed over rather than failing the run.
 */
export function traces(library: string): Map<string, string> {
  const found = new Map<string, string>()
  const walk = (path: string) => {
    let stat
    try {
      stat = lstatSync(path)
    } catch {
      return
    }
    found.set(path, `${stat.isDirectory() ? 'dir' : stat.size} ${stat.mtimeMs}`)
    if (stat.isDirectory()) {
      for (const name of list(path)) walk(join(path, name))
    }
  }
  for (const top of list(library)) {
    const folder = join(library, top)
    if (OURS.test(top)) walk(folder)
    for (const name of list(folder)) {
      if (OURS.test(name)) walk(join(folder, name))
    }
  }
  return found
}

function list(path: string): string[] {
  try {
    return readdirSync(path)
  } catch {
    return []
  }
}

/** What changed between two calls of [`traces`], one line per path, sorted. */
export function changed(before: Map<string, string>, after: Map<string, string>): string[] {
  const lines: string[] = []
  for (const [path, state] of after) {
    const was = before.get(path)
    if (was === undefined) lines.push(`appeared: ${path}`)
    else if (was !== state) lines.push(`changed: ${path}`)
  }
  for (const path of before.keys()) {
    if (!after.has(path)) lines.push(`disappeared: ${path}`)
  }
  return lines.sort()
}

/** A port on the loopback interface that nothing holds at this moment. */
export async function freePort(): Promise<number> {
  return await new Promise((resolve, reject) => {
    const server = createServer()
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      server.close(() => {
        if (address && typeof address === 'object') resolve(address.port)
        else reject(new Error('no port was given'))
      })
    })
  })
}

/** The processes this one started whose command is `binary`. */
export function childrenRunning(binary: string): number[] {
  const out = execFileSync('ps', ['-A', '-o', 'pid=,ppid=,args='], { encoding: 'utf8' })
  return out
    .split('\n')
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/))
    .filter((m): m is RegExpMatchArray => m !== null)
    .filter((m) => Number(m[2]) === process.pid && m[3].split(' ')[0].endsWith(binary))
    .map((m) => Number(m[1]))
}

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

/**
 * Stops the application this run started and waits until it is gone, so that
 * whatever it writes on its way out – the window state is saved on exit – is
 * written before the real `Library` is looked at again.
 */
export async function stopApplication(binary: string, limitMs = 10_000): Promise<void> {
  const pids = childrenRunning(binary)
  for (const pid of pids) process.kill(pid, 'SIGTERM')
  const deadline = Date.now() + limitMs
  while (pids.some(alive)) {
    if (Date.now() > deadline) {
      throw new Error(
        `Aruna E2E: the application did not exit within ${limitMs} ms (${pids.join(', ')})`,
      )
    }
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
}

/** A run of the E2E contour, in a home of its own. */
export interface Isolation {
  /** The directory that stands in for the home, `TMPDIR` and `Library` of the application. */
  root: string
  /** The real `Library` before the run. */
  before: Map<string, string>
  /** WebKit's temporary folder of the application, and whether it was there before the run. */
  webkitTemporary: string
  webkitTemporaryBefore: boolean
}

/**
 * Gives the application a home of its own for this run: `HOME` and
 * `CFFIXED_USER_HOME` (macOS takes WebKit data, caches and saved state from the
 * second, the core and the window-state plugin from the first) and `TMPDIR`.
 * Takes a picture of the real `Library` to compare with afterwards.
 *
 * One folder is not moved by any of them: WebKit makes
 * `aruna-desktop/WebKit/{MediaCache,ModelElement}` in the user's temporary
 * directory of the system (`getconf DARWIN_USER_TEMP_DIR`), not in `TMPDIR` –
 * measured 2026-10-11, the first run with `TMPDIR` set still made it there. It
 * is removed after the run instead, when the run is what made it.
 */
export function isolate(
  env: Record<string, string>,
  library = join(homedir(), 'Library'),
  systemTemporary = tmpdir(),
): Isolation {
  const root = mkdtempSync(join(tmpdir(), 'aruna-e2e-'))
  const home = join(root, 'home')
  const temporary = join(root, 'tmp')
  mkdirSync(home)
  mkdirSync(temporary)
  env.HOME = home
  env.CFFIXED_USER_HOME = home
  env.TMPDIR = `${temporary}/`
  const webkitTemporary = join(systemTemporary, 'aruna-desktop')
  return {
    root,
    before: traces(library),
    webkitTemporary,
    webkitTemporaryBefore: existsSync(webkitTemporary),
  }
}

/**
 * Ends the run: the application stopped, its home and WebKit's temporary
 * folder removed, and the real `Library` compared with the picture taken
 * before. Anything of Aruna's that appeared or changed there is a failure of
 * the run, named path by path.
 */
export async function finish(
  run: Isolation,
  binary: string,
  library = join(homedir(), 'Library'),
): Promise<void> {
  try {
    await stopApplication(binary)
  } finally {
    rmSync(run.root, { recursive: true, force: true })
    if (!run.webkitTemporaryBefore) rmSync(run.webkitTemporary, { recursive: true, force: true })
  }
  const lines = changed(run.before, traces(library))
  if (lines.length > 0) {
    throw new Error(`Aruna E2E wrote into the real Library:\n${lines.join('\n')}`)
  }
}
