import { readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

/**
 * The Rust tests that go to loopback are the ones `docs/TESTING.md` lists.
 *
 * Under the no-network profile of the acceptance runs (`nonet.sb`) a connection
 * to 127.0.0.1 is now and then refused, and the tests that open one go red
 * without a defect of theirs. The owner's decision of 2026-10-15: they are
 * neither retried nor taken out of the run; a red offline run is read against
 * a list of them, with its number. A list nobody checks goes stale the day a
 * download test is added, so this holds it to the sources.
 *
 * The signals are the ones the list was made by. A test opens a socket when its
 * body calls `TcpListener::bind`, `TcpStream::connect` or `UdpSocket::bind`, or
 * reaches a helper that does: a function of the same file – or of
 * `cli/tests/support/mod.rs`, for a test binary that declares `mod support` –
 * that calls one, or names a type such a function belongs to, or calls another
 * such function. A test that only writes a loopback address down is a
 * candidate too: whether it connects (to a closed port, through the download)
 * or never opens a socket is for a person to say, and the record says it in
 * one table or the other. Comments and string contents are blanked before the
 * calls are looked for, so prose about a socket is not one.
 *
 * No network, no clock: the sources and the document are all it reads.
 */

const REPO = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
const RECORD = 'docs/TESTING.md'
const ROOTS = ['cli/src', 'cli/tests', 'src-tauri/src']
const SUPPORT = 'cli/tests/support/mod.rs'

const SOCKET = /\b(?:TcpListener::bind|TcpStream::connect(?:_timeout)?|UdpSocket::bind)\b/
const ADDRESS = /127\.0\.0\.1|\blocalhost\b|\[::1\]/

const OPENS = /^\*\*(\d+) tests open a socket on loopback\.\*\*$/m
const NAMES = /^\*\*(\d+) tests write a loopback address down and open no socket\.\*\*$/m

type Source = { file: string; text: string }

type Fn = {
  name: string
  test: boolean
  /** The type of the `impl` block it sits in, if any. */
  owner: string | null
  /** The body with comments and string contents blanked. */
  code: string
  /** The body as written. */
  text: string
}

/** A test, as the record and the scan both name it. */
type Key = string

function key(file: string, name: string): Key {
  return `${file} ${name}`
}

/** Comments and the contents of string and char literals as spaces; newlines kept. */
function blank(src: string): string {
  const spaces = (s: string): string => s.replace(/[^\n]/g, ' ')
  let out = ''
  let i = 0
  while (i < src.length) {
    if (src.startsWith('//', i)) {
      const nl = src.indexOf('\n', i)
      const end = nl === -1 ? src.length : nl
      out += spaces(src.slice(i, end))
      i = end
      continue
    }
    if (src.startsWith('/*', i)) {
      let depth = 1
      let j = i + 2
      while (j < src.length && depth > 0) {
        if (src.startsWith('/*', j)) {
          depth++
          j += 2
        } else if (src.startsWith('*/', j)) {
          depth--
          j += 2
        } else j++
      }
      out += spaces(src.slice(i, j))
      i = j
      continue
    }
    const raw = /^b?r(#*)"/.exec(src.slice(i, i + 16))
    if (raw && !/\w/.test(src[i - 1] ?? '')) {
      const close = `"${raw[1]}`
      const j = src.indexOf(close, i + raw[0].length)
      out += raw[0] + spaces(src.slice(i + raw[0].length, j)) + close
      i = j + close.length
      continue
    }
    if (src[i] === '"') {
      let j = i + 1
      while (src[j] !== '"') j += src[j] === '\\' ? 2 : 1
      out += `"${spaces(src.slice(i + 1, j))}"`
      i = j + 1
      continue
    }
    if (src[i] === "'") {
      const ch = /^'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'/.exec(
        src.slice(i, i + 12),
      )
      if (ch) {
        out += `'${' '.repeat(ch[0].length - 2)}'`
        i += ch[0].length
        continue
      }
    }
    out += src[i]
    i++
  }
  return out
}

/** The index just past the brace that closes the one at `open`. */
function closing(code: string, open: number): number {
  let depth = 0
  for (let i = open; i < code.length; i++) {
    if (code[i] === '{') depth++
    else if (code[i] === '}' && --depth === 0) return i + 1
  }
  return code.length
}

/** Every function with a body, and whether `#[test]` is among its attributes. */
function functions(src: string): Fn[] {
  const code = blank(src)
  const lines = src.split('\n')
  // The type of each `impl` block, and the stretch its braces hold: a function
  // is a method only inside them, not anywhere after the `impl` line.
  const impls = [
    ...code.matchAll(/\bimpl\b(?:<[^>]*>)?\s+(?:[\w:]+(?:<[^>]*>)?\s+for\s+)?(\w+)[^{;]*\{/g),
  ].map((m) => {
    const open = m.index + m[0].length - 1
    return { owner: m[1], open, close: closing(code, open) }
  })
  const found: Fn[] = []
  for (const m of code.matchAll(/\bfn\s+(\w+)\s*[<(]/g)) {
    const open = code.indexOf('{', m.index)
    const semi = code.indexOf(';', m.index)
    if (open === -1 || (semi !== -1 && semi < open)) continue
    const end = closing(code, open)
    let test = false
    for (let k = src.slice(0, m.index).split('\n').length - 2; k >= 0; k--) {
      const line = lines[k].trim()
      if (line === '#[test]') test = true
      else if (!/^(#\[.*\]|\/\/.*)$/.test(line)) break
    }
    const owner = impls.filter((b) => b.open < m.index && m.index < b.close).pop()
    found.push({
      name: m[1],
      test,
      owner: owner ? owner.owner : null,
      code: code.slice(open, end),
      text: src.slice(open, end),
    })
  }
  return found
}

function names(code: string, helpers: Set<string>): boolean {
  return [...helpers].some((name) => new RegExp(`(?<![\\w.:])${name}\\(`).test(code))
}

function types(code: string, owners: Set<string>): boolean {
  return [...owners].some((owner) => new RegExp(`\\b${owner}::`).test(code))
}

/**
 * The tests of `sources` that open a socket on loopback, and the ones that only
 * write a loopback address down.
 */
function scan(sources: Source[]): { opens: Set<Key>; names: Set<Key> } {
  const parsed = new Map(sources.map((s) => [s.file, functions(s.text)]))
  const support = parsed.get(SUPPORT) ?? []
  const opens = new Set<Key>()
  const writes = new Set<Key>()
  for (const { file, text } of sources) {
    const own = parsed.get(file) ?? []
    const pool = [...own, ...(file !== SUPPORT && /^\s*mod support;/m.test(text) ? support : [])]
    const helpers = new Set<string>()
    const owners = new Set<string>()
    const marked = new Set<Fn>()
    for (let changed = true; changed;) {
      changed = false
      for (const f of pool) {
        if (f.test || marked.has(f)) continue
        if (SOCKET.test(f.code) || types(f.code, owners) || names(f.code, helpers)) {
          marked.add(f)
          changed = true
          if (f.owner) owners.add(f.owner)
          else helpers.add(f.name)
        }
      }
    }
    for (const f of own.filter((f) => f.test)) {
      if (SOCKET.test(f.code) || types(f.code, owners) || names(f.code, helpers)) {
        opens.add(key(file, f.name))
      } else if (ADDRESS.test(f.text)) {
        writes.add(key(file, f.name))
      }
    }
  }
  return { opens, names: writes }
}

/** The rows of the markdown table right after the line `heading` matches. */
function table(doc: string, heading: RegExp): { count: number; rows: Key[] } | null {
  const at = heading.exec(doc)
  if (!at) return null
  const rows: Key[] = []
  const lines = doc.slice(at.index + at[0].length).split('\n')
  let started = false
  for (const line of lines) {
    if (!line.startsWith('|')) {
      if (started) break
      continue
    }
    started = true
    const cells = line.split('|').map((c) => c.trim())
    const test = /^`([\w:]+)`$/.exec(cells[2] ?? '')
    const file = /^`([\w./-]+\.rs)`$/.exec(cells[3] ?? '')
    if (test && file) rows.push(key(file[1], test[1].split('::').pop() ?? ''))
  }
  return { count: Number(at[1]), rows }
}

/** What is wrong with the record against the scan, one sentence each; empty when nothing is. */
function discrepancies(doc: string, found: { opens: Set<Key>; names: Set<Key> }): string[] {
  const opens = table(doc, OPENS)
  const writes = table(doc, NAMES)
  if (!opens || !writes) {
    return [`${RECORD} has lost the line that heads one of the two loopback tables`]
  }
  const wrong: string[] = []
  const listed = new Set([...opens.rows, ...writes.rows])
  const scanned = new Set([...found.opens, ...found.names])
  const say = (what: string, keys: Key[]): void => {
    if (keys.length > 0) wrong.push(`${what}: ${keys.sort().join(', ')}`)
  }
  say(
    'open a socket on loopback and are not in the first table',
    [...found.opens].filter((k) => !opens.rows.includes(k)),
  )
  say(
    'write a loopback address down and are in neither table',
    [...found.names].filter((k) => !listed.has(k)),
  )
  say(
    'are listed, but no test of that name in that file opens a socket or writes the address',
    [...listed].filter((k) => !scanned.has(k)),
  )
  say(
    'are listed twice',
    [...opens.rows, ...writes.rows].filter((k, i, all) => all.indexOf(k) !== i),
  )
  if (opens.count !== opens.rows.length) {
    wrong.push(`the first table says ${opens.count} and lists ${opens.rows.length}`)
  }
  if (writes.count !== writes.rows.length) {
    wrong.push(`the second table says ${writes.count} and lists ${writes.rows.length}`)
  }
  return wrong.map((w) => `${w} – correct ${RECORD}, "Tests that go to loopback"`)
}

function rustSources(): Source[] {
  const walk = (dir: string): string[] =>
    readdirSync(dir)
      .sort()
      .flatMap((name) => {
        const path = join(dir, name)
        return statSync(path).isDirectory() ? walk(path) : path.endsWith('.rs') ? [path] : []
      })
  return ROOTS.flatMap((root) => walk(join(REPO, root))).map((path) => ({
    file: relative(REPO, path),
    text: readFileSync(path, 'utf8'),
  }))
}

describe('the record of loopback tests', () => {
  it(`is the set the Rust sources give, with its numbers, in ${RECORD}`, () => {
    const doc = readFileSync(join(REPO, RECORD), 'utf8')
    expect(discrepancies(doc, scan(rustSources()))).toEqual([])
  })

  it('names what is missing, what is made up and a wrong number', () => {
    const sources: Source[] = [
      {
        file: 'cli/tests/net.rs',
        text: [
          'mod support;',
          'struct Server;',
          'impl Server {',
          '    fn start() -> Self { let _l = std::net::TcpListener::bind("127.0.0.1:0"); Server }',
          '}',
          'fn serve() -> Server { Server::start() }',
          '#[test]',
          'fn listens() { let _l = std::net::TcpListener::bind("x"); }',
          '/// TcpStream::connect, in prose only.',
          '#[test]',
          'fn through_a_type() { let _s = Server::start(); }',
          '#[test]',
          'fn through_a_function() { let _s = serve(); }',
          '#[test]',
          'fn through_support() { let _o = Origin::serving(vec![]); }',
          '#[test]',
          'fn writes_it_down() { let _u = "http://127.0.0.1:1/"; }',
          '#[test]',
          'fn talks_about_it() { let _t = "TcpListener::bind"; }',
        ].join('\n'),
      },
      {
        file: SUPPORT,
        text: [
          'pub struct Origin;',
          'impl Origin {',
          '    pub fn serving(_b: Vec<u8>) -> Self { let _ = std::net::TcpStream::connect("x"); Origin }',
          '}',
        ].join('\n'),
      },
    ]
    const found = scan(sources)
    expect([...found.opens].sort()).toEqual([
      'cli/tests/net.rs listens',
      'cli/tests/net.rs through_a_function',
      'cli/tests/net.rs through_a_type',
      'cli/tests/net.rs through_support',
    ])
    expect([...found.names]).toEqual(['cli/tests/net.rs writes_it_down'])

    const record = (opens: string[], count: number): string =>
      [
        `**${count} tests open a socket on loopback.**`,
        '',
        '| binary | test | file | opens | through |',
        '|---|---|---|---|---|',
        ...opens.map((t) => `| \`aruna::net\` | \`${t}\` | \`cli/tests/net.rs\` | listens | – |`),
        '',
        '**1 tests write a loopback address down and open no socket.**',
        '',
        '| binary | test | file | opens | through |',
        '|---|---|---|---|---|',
        '| `aruna::net` | `writes_it_down` | `cli/tests/net.rs` | none | – |',
        '',
      ].join('\n')
    const all = ['listens', 'through_a_type', 'through_a_function', 'through_support']
    expect(discrepancies(record(all, 4), found)).toEqual([])

    const dropped = discrepancies(record(all.slice(1), 3), found)
    expect(dropped).toHaveLength(1)
    expect(dropped[0]).toMatch(
      /not in the first table: cli\/tests\/net\.rs listens – correct docs\/TESTING\.md/,
    )

    const madeUp = discrepancies(record([...all, 'no_such_test'], 5), found)
    expect(madeUp).toHaveLength(1)
    expect(madeUp[0]).toMatch(/are listed, but .*: cli\/tests\/net\.rs no_such_test/)

    const miscounted = discrepancies(record(all, 5), found)
    expect(miscounted).toEqual([
      'the first table says 5 and lists 4 – correct docs/TESTING.md, "Tests that go to loopback"',
    ])
  })
})
