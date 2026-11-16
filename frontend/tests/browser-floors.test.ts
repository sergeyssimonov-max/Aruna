import { expect, it } from 'vitest'
import { build, loadConfigFromFile, resolveConfig, type InlineConfig } from 'vite'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { inventoryBuild } from '../build/inventory.ts'

const windowConfig = fileURLToPath(new URL('../vite.config.ts', import.meta.url))

// Exercise lowering, not only the spelling of a target. Optional chaining and
// nullish coalescing need lowering for Safari 13.0, but not Safari 16.0.
async function probe(config: InlineConfig, dir: string): Promise<string> {
  const entry = join(dir, 'probe.js')
  await writeFile(join(dir, 'probe.css'), '.aruna-floor { & .child { appearance: none; } }')
  await writeFile(
    entry,
    'import "./probe.css"; globalThis.arunaFloorProbe = (value) => value?.nested ?? "fallback";\n',
  )
  const result = await build({
    ...config,
    configFile: false,
    publicDir: false,
    logLevel: 'silent',
    build: {
      ...config.build,
      outDir: dir,
      emptyOutDir: false,
      rolldownOptions: { input: entry },
      lib: { entry, formats: ['iife'], name: 'ArunaFloorProbe', cssFileName: 'floor-probe' },
    },
  })
  const output = Array.isArray(result) ? result[0] : result
  if (!output || !('output' in output)) throw new Error('Expected a completed probe build')
  return output.output
    .filter((item) => item.type === 'chunk')
    .map((item) => item.code)
    .join('\n')
}

it('keeps the window JS and CSS at Safari 16.0', async () => {
  const loaded = await loadConfigFromFile({ command: 'build', mode: 'production' }, windowConfig)
  if (!loaded) throw new Error('Window configuration was not loaded')
  const resolved = await resolveConfig({ ...loaded.config, configFile: false }, 'build')
  expect(resolved.build.target).toBe('safari16')
  expect(resolved.build.cssTarget).toBe('safari16')
  const dir = await mkdtemp(join(tmpdir(), 'aruna-window-floor-'))
  try {
    const js = await probe(loaded.config, dir)
    expect(js).toContain('arunaFloorProbe')
    expect(js).toContain('?.')
    expect(js).toContain('??')
    const css = await readFile(join(dir, 'floor-probe.css'), 'utf8')
    expect(css).toContain('.aruna-floor .child')
    expect(css).not.toContain('&')
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
})

it('lowers the inventory to Safari 13.0 and retains its required CSS prefix', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'aruna-inventory-floor-'))
  try {
    const config = inventoryBuild(dir)
    const resolved = await resolveConfig(config, 'build')
    expect(resolved.build.target).toBe('safari13')
    expect(resolved.build.cssTarget).toBe('safari13')
    const js = await probe(config, dir)
    expect(js).toContain('arunaFloorProbe')
    expect(js).not.toContain('?.')
    expect(js).not.toContain('??')
    expect(js).toContain('fallback')
    expect(await readFile(join(dir, 'screen.css'), 'utf8')).toContain('-webkit-appearance:')
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
})
