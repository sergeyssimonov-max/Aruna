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
// node_modules, with exact-version references to verified local upstream
// texts in scripts/third-party-sources. A package without an available or
// reviewed text is marked "requires review" and listed. The set of crates is
// Cargo.lock, which cargo
// resolves with every feature of the workspace; the script checks it against
// `cargo metadata --all-features` before it writes.
// Both pnpm stores must contain installed locked packages. Otherwise an
// uninstalled checkout could replace all npm licence texts with review rows.
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

// Offline, exact-version references reviewed against registry archives or the
// publishing commit. Never extend a reference to an unreviewed version.
const npmReviewed = JSON.parse(readFileSync(join(ROOT, 'scripts/third-party-sources/npm-reviewed.json'), 'utf8'))
const npmReferences = new Map()
function npmFiles(lockDir, name, version) {
  const dir = npmDir(lockDir, name, version)
  const manifest = dir ? JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')) : {}
  const declared =
    typeof manifest.license === 'string'
      ? manifest.license
      : (manifest.license?.type ?? manifest.licenses?.map((l) => l.type ?? l).join(' OR ') ?? null)
  const files = dir ? licenceFiles(dir) : []
  const reference = npmReviewed.find((r) => r.name === name && r.version === version)
  if (!reference) return { declared, files, where: dir ? 'node_modules' : null }
  if (dir && declared !== reference.license) throw new Error(`reviewed npm licence declaration changed: ${name} ${version}`)
  const verified = reference.files.map(({ file, sha256 }) => {
    const path = join(ROOT, 'scripts/third-party-sources', file)
    const bytes = existsSync(path) ? readFileSync(path) : null
    if (!bytes || createHash('sha256').update(bytes).digest('hex') !== sha256) {
      throw new Error(`verified npm licence is missing or changed: ${file}`)
    }
    return [file, bytes]
  })
  // Installed texts remain authoritative: do not conceal unexpected changes.
  for (const [file, bytes] of files) {
    const digest = createHash('sha256').update(bytes).digest('hex')
    if (!reference.files.some((f) => f.sha256 === digest)) throw new Error(`installed npm licence differs from review: ${name} ${file}`)
  }
  npmReferences.set(`${name} ${version}`, reference)
  return { declared: reference.license, files: verified, where: 'verified source' }
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

const npmLocks = [
  [join(ROOT, 'frontend', 'pnpm-lock.yaml'), 'npm', 'pnpm --dir frontend install --frozen-lockfile'],
  [join(ROOT, 'pnpm-lock.yaml'), 'npm (root)', 'pnpm install --frozen-lockfile'],
]
// Check before reading the registry or writing anything. Missing platform
// packages are expected; an entire missing installation is not.
for (const [lock, , install] of npmLocks) {
  if (!npmPackages(lock).some(([name, version]) => npmDir(dirname(lock), name, version))) {
    console.error(`third-party-notices: no installed locked npm packages in ${join(dirname(lock), 'node_modules', '.pnpm')}; run \`${install}\` before generating notices`)
    process.exit(1)
  }
}

const rows = []
const texts = new Map()
for (const [name, version] of crates()) rows.push({ eco: 'crate', name, version, ...crateFiles(name, version) })
for (const [lock, label] of npmLocks) {
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
// selectors 0.38.0 and 0.41.0 declare MPL-2.0 and their source headers point to Mozilla,
// but the published crate contains no licence file. This is a reviewed,
// version-specific reference to the exact MPL text shipped by cssparser 0.38.0;
// do not treat missing crates or unknown versions as reviewed.
const selectorsSources = new Map([
  ['0.38.0', '572ecba2d1600e7c3d490586692a209faf703baa'],
  ['0.41.0', '34d25f741765a0bdb894f8271852e633299352d8'],
])
const mplDigest = 'fab3dd6bdab226f1c08630b1dd917e11fcb4ec5e1e020e2c16f83a0a13863e85'
const selectorsReferences = []
for (const selectors of rows.filter((r) => r.eco === 'crate' && r.name === 'selectors' && selectorsSources.has(r.version))) {
  if (!selectors.where || selectors.declared !== 'MPL-2.0' || selectors.files.length !== 0) continue
  const source = rows.find((r) => r.eco === 'crate' && r.name === 'cssparser' && r.version === '0.38.0')
  const licence = source?.files.find(([name]) => name === 'LICENSE')
  if (source?.declared !== 'MPL-2.0' || !licence || createHash('sha256').update(licence[1]).digest('hex') !== mplDigest) {
    console.error('third-party-notices: verified MPL-2.0 reference is missing or changed')
    process.exit(1)
  }
  selectors.refs = [mplDigest]
  selectorsReferences.push(selectors)
}

// These crates share one reviewed upstream commit. The published crates omit
// its root LICENSE and NOTICE. Keep the upstream NOTICE byte for byte and
// verify both it and the identical LICENSE from typst-assets before use.
const typstNames = new Set(['typst', 'typst-eval', 'typst-html', 'typst-layout', 'typst-library', 'typst-macros', 'typst-pdf', 'typst-realize', 'typst-svg', 'typst-syntax', 'typst-timing', 'typst-utils'])
const typstLicenseDigest = '62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a'
const typstNoticeDigest = '1778244777547c281b6f5fa9fc0c18ab21f8d4491c803f64e09046800f5fcb26'
const typstReferences = []
for (const row of rows.filter((r) => r.eco === 'crate' && typstNames.has(r.name) && r.version === '0.15.1')) {
  if (!row.where || row.declared !== 'Apache-2.0' || row.files.length !== 0) continue
  const source = rows.find((r) => r.eco === 'crate' && r.name === 'typst-assets' && r.version === '0.15.1')
  const licence = source?.files.find(([name]) => name === 'LICENSE')
  const noticePath = join(ROOT, 'scripts/third-party-sources/typst-0.15.1-NOTICE')
  const notice = existsSync(noticePath) ? readFileSync(noticePath) : null
  if (source?.declared !== 'Apache-2.0' || !licence || createHash('sha256').update(licence[1]).digest('hex') !== typstLicenseDigest || !notice || createHash('sha256').update(notice).digest('hex') !== typstNoticeDigest) {
    console.error('third-party-notices: verified Typst licence or upstream NOTICE is missing or changed')
    process.exit(1)
  }
  if (!texts.has(typstNoticeDigest)) texts.set(typstNoticeDigest, { bytes: notice, users: ['Typst upstream NOTICE, commit 9dfd3a08500b7896045f907433cf7b4b02434fad'] })
  row.refs = [typstLicenseDigest, typstNoticeDigest]
  typstReferences.push(row)
}

// The upstream policy confirms SPDX declarations but flags uncertainty about
// Apple SDK-derived code and does not itself contain the full MIT text. Keep
// those packages in review while preserving the original upstream policy.
const objc2Versions = new Map([
  ['block2', '0.6.2'], ['dispatch2', '0.3.1'], ['objc2', '0.6.5'],
  ['objc2-encode', '4.1.0'], ['objc2-exception-helper', '0.1.1'],
  ...['app-kit', 'cloud-kit', 'core-data', 'core-foundation', 'core-graphics', 'core-image', 'core-location', 'core-text', 'core-video', 'foundation', 'io-surface', 'javascript-core', 'quartz-core', 'security', 'ui-kit', 'user-notifications', 'web-kit'].map((name) => [`objc2-${name}`, '0.3.2']),
])
const objc2PolicyDigest = '7f976f7e9cb2d87df7230606feb932c3f21ac0e664045a775b600046ff850c54'
const objc2Reviews = rows.filter((r) => r.eco === 'crate' && objc2Versions.get(r.name) === r.version && r.refs.length === 0)
if (objc2Reviews.length) {
  const path = join(ROOT, 'scripts/third-party-sources/objc2-LICENSE.md')
  const policy = existsSync(path) ? readFileSync(path) : null
  if (!policy || createHash('sha256').update(policy).digest('hex') !== objc2PolicyDigest) {
    console.error('third-party-notices: verified objc2 upstream policy is missing or changed')
    process.exit(1)
  }
  if (!texts.has(objc2PolicyDigest)) texts.set(objc2PolicyDigest, { bytes: policy, users: ['objc2 upstream licence policy'] })
}

// Each Specta repository has its own reviewed MIT licence and commit. Their
// texts differ by a final newline, so retain and verify both byte sequences.
const spectaGroups = [
  { versions: new Map([['specta', '2.0.0-rc.25'], ['specta-macros', '2.0.0-rc.25'], ['specta-serde', '0.0.12'], ['specta-typescript', '0.0.12'], ['specta-util', '0.0.12']]), file: 'specta-e7fd23f-LICENSE', digest: '9ec86f39e235fcd35e6d91ef3fd4d6b88e5763f3ce1e296c222969986b6f6475', source: 'https://github.com/specta-rs/specta/blob/e7fd23f586b9bd58de39002dabe49b1f978bf971/LICENSE', finalNewline: false },
  { versions: new Map([['tauri-specta', '2.0.0-rc.25'], ['tauri-specta-macros', '2.0.0-rc.25']]), file: 'tauri-specta-e5e578f-LICENSE', digest: '79b0c7d284efc4c2dc8da73c01a6d10c19caf61e70fca813e642dc010049eee7', source: 'https://github.com/specta-rs/tauri-specta/blob/e5e578fb339fc2b0d6c676b3031c3dafcec88083/LICENSE', finalNewline: true },
]
for (const group of spectaGroups) {
  group.reviewed = []
  for (const row of rows.filter((r) => r.eco === 'crate' && group.versions.get(r.name) === r.version)) {
    if (!row.where || row.declared !== 'MIT' || row.files.length !== 0) continue
    const path = join(ROOT, 'scripts/third-party-sources', group.file)
    const licence = existsSync(path) ? readFileSync(path) : null
    if (!licence || createHash('sha256').update(licence).digest('hex') !== group.digest) {
      console.error(`third-party-notices: verified ${group.file} is missing or changed`)
      process.exit(1)
    }
    if (!texts.has(group.digest)) texts.set(group.digest, { bytes: licence, users: [`upstream ${group.file}`] })
    row.refs = [group.digest]
    group.reviewed.push(row)
  }
}

// Stable, reviewed baseline ordering avoids moving unchanged verbatim licence
// text when the number of users changes. Unknown texts follow in hash order.
// Append reviewed new hashes to the baseline; do not reorder existing entries.
const baseline = JSON.parse(readFileSync(join(ROOT, 'scripts/third-party-license-order.json'), 'utf8'))
if (!Array.isArray(baseline) || baseline.some((d) => typeof d !== 'string' || !/^[0-9a-f]{64}$/.test(d)) || new Set(baseline).size !== baseline.length) {
  throw new Error('invalid third-party licence order')
}
const positions = new Map(baseline.map((d, i) => [d, i]))
const order = [...texts.keys()].sort((a, b) => (positions.get(a) ?? Infinity) - (positions.get(b) ?? Infinity) || (a < b ? -1 : 1))
const label = new Map(order.map((d, i) => [d, `T${i + 1}`]))
const review = rows.filter((r) => r.refs.length === 0)
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
  'Texts below are copied byte for byte from the local Cargo registry, installed npm packages,',
  'or documented, verified upstream sources.',
  'A documented reference may use another package’s identical standard licence text. A package',
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
for (const selectors of selectorsReferences) {
  out.push('', `selectors ${selectors.version} declares MPL-2.0 but ships no licence file. Its reference ${label.get(mplDigest)} is the unmodified MPL-2.0 text from cssparser 0.38.0 (LICENSE); its source headers point to https://mozilla.org/MPL/2.0/. The selectors source is https://github.com/servo/stylo/tree/${selectorsSources.get(selectors.version)}/selectors.`, '')
}
if (typstReferences.length) {
  out.push('', `Typst ${typstReferences.map((r) => `${r.name} ${r.version}`).join(', ')} declare Apache-2.0 but ship no licence file. Reference ${label.get(typstLicenseDigest)} is the unmodified LICENSE from typst-assets 0.15.1, verified identical to the upstream LICENSE; ${label.get(typstNoticeDigest)} is the unmodified upstream NOTICE. Both originate from https://github.com/typst/typst/tree/9dfd3a08500b7896045f907433cf7b4b02434fad.`, '')
}
if (objc2Reviews.length) {
  out.push('', `For ${objc2Reviews.map((r) => `${r.name} ${r.version}`).join(', ')}, ${label.get(objc2PolicyDigest)} preserves the upstream licence policy from https://github.com/madsmtm/objc2/blob/d7d2fa23ceaa5e6096c923b081040e5d81b3b9df/LICENSE.md (identical at all five reviewed source commits). It identifies the declared licences, but does not provide the complete MIT text and notes uncertainty concerning Apple SDK-derived code. These packages remain listed for review.`, '')
}
for (const group of spectaGroups.filter((g) => g.reviewed.length)) {
  out.push('', `${group.reviewed.map((r) => `${r.name} ${r.version}`).join(', ')} declare MIT but ship no licence file. Reference ${label.get(group.digest)} is the unmodified upstream LICENSE from ${group.source}. The source file ${group.finalNewline ? 'ends' : 'does not end'} with a newline; Markdown adds a line break before the closing fence.`, '')
}
for (const [name, reference] of npmReferences) {
  out.push('', `${name}: reviewed exact-version MIT reference; original texts preserved from ${reference.files.map((f) => f.source).join(', ')}.`, '')
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
