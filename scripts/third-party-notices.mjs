#!/usr/bin/env node
// Writes THIRD-PARTY-NOTICES.md: every package of the three lock files, its
// declared licence, and the licence texts the package itself ships.
//
//   node scripts/third-party-notices.mjs            # write the file
//   node scripts/third-party-notices.mjs --check    # compare, change nothing
//
// Owner's decision of 2026-10-02: a notice file for the third-party licences,
// before the release with PDF. Nothing here is written by hand and nothing is
// fetched: a text is copied byte for byte from the local Cargo registry (the
// unpacked crate, or the .crate archive when it was never unpacked) or from
// node_modules. A package whose files are not on this machine is marked
// "requires review" and listed. The set of crates is Cargo.lock, which cargo
// resolves with every feature of the workspace; the script checks it against
// `cargo metadata --all-features` before it writes.
//
// The coverage of the file – every locked package named, nothing else – is
// held by frontend/tests/third-party-notices.test.ts, which reads only the
// lock files and this file, so it runs where no registry is.

import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const OUT = join(ROOT, 'THIRD-PARTY-NOTICES.md')
const LICENCE = /^(licen[cs]e|copying|copyright|notice|unlicense|authors)([-._].*)?$/i
const REGISTRY = join(process.env.CARGO_HOME ?? join(homedir(), '.cargo'), 'registry')

/** `[name, version]` of every registry crate in Cargo.lock. */
function crates() {
  const out = []
  for (const block of readFileSync(join(ROOT, 'Cargo.lock'), 'utf8').split('[[package]]').slice(1)) {
    const field = (k) => block.match(new RegExp(`^${k} = "([^"]*)"`, 'm'))?.[1]
    if (field('source')?.startsWith('registry+')) out.push([field('name'), field('version')])
  }
  return out
}

/** `name@version` keys of the `packages:` section of a pnpm lock file. */
function npmPackages(lock) {
  const lines = readFileSync(lock, 'utf8').split('\n')
  const start = lines.indexOf('packages:')
  const out = []
  if (start === -1) return out
  for (const line of lines.slice(start + 1)) {
    if (/^\S/.test(line)) break
    const m = line.match(/^ {2}'?([^' ][^']*?)'?:$/)
    if (!m) continue
    const at = m[1].lastIndexOf('@')
    out.push([m[1].slice(0, at), m[1].slice(at + 1)])
  }
  return out
}

/** The licence files of a directory: `[file name, bytes]`, by name. */
function licenceFiles(dir) {
  return readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && LICENCE.test(e.name))
    .map((e) => [e.name, readFileSync(join(dir, e.name))])
    .sort((a, b) => (a[0] < b[0] ? -1 : 1))
}

function crateFiles(name, version) {
  const id = `${name}-${version}`
  for (const index of readdirSync(join(REGISTRY, 'src'))) {
    const dir = join(REGISTRY, 'src', index, id)
    if (existsSync(join(dir, 'Cargo.toml'))) {
      return { declared: crateLicence(readFileSync(join(dir, 'Cargo.toml'), 'utf8')), files: licenceFiles(dir), where: 'registry' }
    }
  }
  for (const index of readdirSync(join(REGISTRY, 'cache'))) {
    const archive = join(REGISTRY, 'cache', index, `${id}.crate`)
    if (!existsSync(archive)) continue
    const entries = execFileSync('tar', ['-tzf', archive], { encoding: 'utf8' }).split('\n')
    const top = entries
      .filter((e) => e.startsWith(`${id}/`) && !e.slice(id.length + 1).includes('/'))
      .map((e) => e.slice(id.length + 1))
    const read = (f) => execFileSync('tar', ['-xzOf', archive, `${id}/${f}`], { maxBuffer: 1 << 26 })
    const files = top
      .filter((f) => LICENCE.test(f))
      .sort()
      .map((f) => [f, read(f)])
    return { declared: crateLicence(read('Cargo.toml').toString('utf8')), files, where: '.crate' }
  }
  return { declared: null, files: [], where: null }
}

function crateLicence(toml) {
  const pkg = toml.split(/^\[package\]$/m)[1]?.split(/^\[/m)[0] ?? ''
  const licence = pkg.match(/^license\s*=\s*"([^"]*)"/m)?.[1]
  const file = pkg.match(/^license-file\s*=\s*"([^"]*)"/m)?.[1]
  return licence ?? (file ? `see ${file}` : null)
}

/** The `.pnpm` directories of a lock file's node_modules, read once. */
const stores = new Map()
function storeDirs(lockDir) {
  const store = join(lockDir, 'node_modules', '.pnpm')
  if (!stores.has(store)) stores.set(store, existsSync(store) ? readdirSync(store) : [])
  return [store, stores.get(store)]
}

/** The installed copy of `name@version`: pnpm names its directory after the
 *  package and adds the peers it was resolved with after a `_`. */
function npmDir(lockDir, name, version) {
  const [store, dirs] = storeDirs(lockDir)
  const base = `${name.replace('/', '+')}@${version}`
  for (const d of dirs.filter((d) => d === base || d.startsWith(`${base}_`)).sort()) {
    const dir = join(store, d, 'node_modules', name)
    if (existsSync(join(dir, 'package.json'))) {
      const manifest = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'))
      if (manifest.version === version) return dir
    }
  }
  return null
}

function npmFiles(lockDir, name, version) {
  const dir = npmDir(lockDir, name, version)
  if (!dir) return { declared: null, files: [], where: null }
  const manifest = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'))
  const declared =
    typeof manifest.license === 'string'
      ? manifest.license
      : (manifest.license?.type ?? manifest.licenses?.map((l) => l.type ?? l).join(' OR ') ?? null)
  return { declared, files: licenceFiles(dir), where: 'node_modules' }
}

/** cargo metadata --all-features: the registry crates it resolves. */
function metadataCrates() {
  const json = execFileSync('cargo', ['metadata', '--locked', '--offline', '--all-features', '--format-version', '1'], {
    cwd: ROOT,
    maxBuffer: 1 << 28,
    encoding: 'utf8',
  })
  return JSON.parse(json)
    .packages.filter((p) => p.source?.startsWith('registry+'))
    .map((p) => `${p.name} ${p.version}`)
}

const rows = []
const texts = new Map()
for (const [name, version] of crates()) rows.push({ eco: 'crate', name, version, ...crateFiles(name, version) })
for (const [lock, label] of [
  [join(ROOT, 'frontend', 'pnpm-lock.yaml'), 'npm'],
  [join(ROOT, 'pnpm-lock.yaml'), 'npm (root)'],
]) {
  for (const [name, version] of npmPackages(lock)) rows.push({ eco: label, name, version, ...npmFiles(dirname(lock), name, version) })
}

const locked = new Set(crates().map(([n, v]) => `${n} ${v}`))
const resolved = new Set(metadataCrates())
const onlyLock = [...locked].filter((k) => !resolved.has(k))
const onlyMeta = [...resolved].filter((k) => !locked.has(k))
if (onlyLock.length || onlyMeta.length) {
  console.error('Cargo.lock and cargo metadata --all-features differ:', { onlyLock, onlyMeta })
  process.exit(1)
}

for (const row of rows) {
  row.refs = row.files.map(([file, bytes]) => {
    const digest = createHash('sha256').update(bytes).digest('hex')
    if (!texts.has(digest)) texts.set(digest, { bytes, users: [] })
    texts.get(digest).users.push(`${row.name} ${row.version} (${file})`)
    return digest
  })
}
const order = [...texts.keys()].sort((a, b) => texts.get(b).users.length - texts.get(a).users.length || (a < b ? -1 : 1))
const label = new Map(order.map((d, i) => [d, `T${i + 1}`]))
const review = rows.filter((r) => r.files.length === 0)
const cell = (s) => String(s ?? 'not declared').replaceAll('|', '\\|')

const out = []
out.push('# Third-party notices', '')
out.push(
  'Aruna – the application `Aruna.app` and the console program `aruna` – is built from the',
  'third-party packages below. This file names every package of the three lock files of the',
  'repository, `Cargo.lock`, `frontend/pnpm-lock.yaml` and `pnpm-lock.yaml`, with the licence it',
  'declares and the licence texts it ships. It covers what is only used to build, lint and test',
  'and the platform packages of other systems as well; what ends up in the application is a',
  'subset of it. The crates are the set Cargo resolves with every feature of the workspace',
  '(`cargo metadata --all-features`), which is the set of `Cargo.lock`.',
  '',
  'Every text below is copied byte for byte from the files the package itself ships – the local',
  'Cargo registry for crates, `node_modules` for npm packages. None was written by hand. A package',
  'whose files were not available without the network is marked *requires review* and listed',
  'under that heading. Identical texts are given once. The file is written by',
  '`scripts/third-party-notices.mjs` (owner\'s decision of 2026-10-02); its coverage of the lock',
  'files is checked by `frontend/tests/third-party-notices.test.ts`. It ships inside the',
  'application, in `Aruna.app/Contents/Resources/`, and is not part of the package Aruna builds.',
  '',
)
out.push(`Packages: ${rows.length} – crates ${rows.filter((r) => r.eco === 'crate').length}, npm ${rows.filter((r) => r.eco !== 'crate').length}. Distinct licence texts: ${texts.size}. Requires review: ${review.length}.`, '')
out.push('## Packages', '', '| Package | Version | From | Declared licence | Texts |', '|---|---|---|---|---|')
for (const r of rows) {
  const t = r.refs.length ? r.refs.map((d) => label.get(d)).join(', ') : 'requires review'
  out.push(`| ${cell(r.name)} | ${cell(r.version)} | ${r.eco} | ${cell(r.declared)} | ${t} |`)
}
out.push('', '## Requires review', '')
out.push(
  'No licence file of these packages was available on the machine that wrote this file: either',
  'the package is not installed here (a platform package of another system, a crate never',
  'downloaded) or it ships no licence file. The declared licence, where known, is in the table.',
  '',
)
for (const r of review) out.push(`- ${r.name} ${r.version} (${r.eco}${r.where ? `, ${r.where}` : ', not on this machine'}): ${r.declared ?? 'not declared'}`)
out.push('', '## Licence texts', '')
for (const d of order) {
  const { bytes, users } = texts.get(d)
  const text = bytes.toString('utf8')
  const longest = Math.max(2, ...[...text.matchAll(/`+/g)].map((m) => m[0].length))
  const fence = '`'.repeat(longest + 1)
  out.push(`### ${label.get(d)}`, '', `SHA-256 \`${d}\`. Used by: ${users.join(', ')}.`, '', `${fence}text`, text.replace(/\n$/, ''), fence, '')
}
const written = out.join('\n')

if (process.argv.includes('--check')) {
  const current = existsSync(OUT) ? readFileSync(OUT, 'utf8') : ''
  if (current !== written) {
    console.error('THIRD-PARTY-NOTICES.md is not what the lock files and the local files give')
    process.exit(1)
  }
  console.log('THIRD-PARTY-NOTICES.md is current')
} else {
  writeFileSync(OUT, written)
  console.log(`${rows.length} packages, ${texts.size} texts, ${review.length} requires review → ${OUT}`)
}
