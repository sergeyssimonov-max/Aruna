import { createRequire } from 'node:module'
import { describe, expect, it } from 'vitest'

type Position = { line: number; column: number }
type Mapping = { generated: Position; original: Position; source: string }
type OriginalPosition = { source: string | null; line: number | null; column: number | null }
type GeneratedPosition = { line: number | null; column: number | null }
interface Generator {
  addMapping(mapping: Mapping): void
  setSourceContent(source: string, content: string): void
  toJSON(): unknown
}
interface Consumer {
  originalPositionFor(position: Position): OriginalPosition
  generatedPositionFor(position: Position & { source: string }): GeneratedPosition
  allGeneratedPositionsFor(position: Position & { source: string }): GeneratedPosition[]
  eachMapping(callback: () => void, context: null, order: number): void
}

const project = createRequire(import.meta.url)
const vite = createRequire(project.resolve('vite'))
const postcss = createRequire(vite.resolve('postcss'))
const { SourceMapGenerator, SourceMapConsumer } = postcss('source-map-js') as {
  SourceMapGenerator: new () => Generator
  SourceMapConsumer: { new (map: unknown): Consumer; ORIGINAL_ORDER: number }
}

function sectionMap(): unknown {
  const map = new SourceMapGenerator()
  for (const line of [1, 2]) {
    map.addMapping({
      generated: { line, column: 0 },
      original: { line, column: 0 },
      source: 'src/опись.css',
    })
  }
  map.setSourceContent('src/опись.css', 'a {}\nb {}')
  return map.toJSON()
}

describe('the source-map-js used by PostCSS', () => {
  it('resolves the reviewed version through the actual build dependency', () => {
    expect(postcss('source-map-js/package.json').version).toBe('1.2.4')
  })

  it('round-trips Unicode source paths and ordinary mappings', () => {
    const consumer = new SourceMapConsumer(sectionMap())
    expect(consumer.originalPositionFor({ line: 2, column: 0 })).toMatchObject({
      source: 'src/опись.css',
      line: 2,
      column: 0,
    })
    expect(
      consumer.generatedPositionFor({ source: 'src/опись.css', line: 2, column: 0 }),
    ).toMatchObject({ line: 2, column: 0 })
  })

  it('applies a section column offset only to its first line', () => {
    const consumer = new SourceMapConsumer({
      version: 3,
      sections: [{ offset: { line: 1, column: 5 }, map: sectionMap() }],
    })
    expect(consumer.originalPositionFor({ line: 2, column: 5 })).toMatchObject({
      source: 'src/опись.css',
      line: 1,
      column: 0,
    })
    expect(
      consumer.generatedPositionFor({ source: 'src/опись.css', line: 1, column: 0 }),
    ).toMatchObject({ line: 2, column: 5 })
    expect(
      consumer.generatedPositionFor({ source: 'src/опись.css', line: 2, column: 0 }),
    ).toMatchObject({ line: 3, column: 0 })
  })

  it('finds positions in nested index maps', () => {
    const consumer = new SourceMapConsumer({
      version: 3,
      sections: [
        {
          offset: { line: 2, column: 0 },
          map: {
            version: 3,
            sections: [{ offset: { line: 1, column: 5 }, map: sectionMap() }],
          },
        },
      ],
    })
    expect(
      consumer.generatedPositionFor({ source: 'src/опись.css', line: 1, column: 0 }),
    ).toMatchObject({ line: 4, column: 5 })
    expect(
      consumer.allGeneratedPositionsFor({ source: 'src/опись.css', line: 1, column: 0 }),
    ).toEqual([expect.objectContaining({ line: 4, column: 5 })])
  })

  it('accepts sources with no mappings after tree shaking', () => {
    const consumer = new SourceMapConsumer({
      version: 3,
      sources: ['unused.ts'],
      names: [],
      mappings: '',
      sourcesContent: ['export const unused = 1'],
    })
    expect(
      consumer.generatedPositionFor({ source: 'unused.ts', line: 1, column: 0 }),
    ).toMatchObject({ line: null, column: null })
    expect(consumer.allGeneratedPositionsFor({ source: 'unused.ts', line: 1, column: 0 })).toEqual(
      [],
    )
    let visited = 0
    consumer.eachMapping(() => visited++, null, SourceMapConsumer.ORIGINAL_ORDER)
    expect(visited).toBe(0)
  })
})
