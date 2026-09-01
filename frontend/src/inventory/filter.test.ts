/**
 * What the inventory's controls do, driven against a document in the shape the
 * Rust renderer writes.
 *
 * The script had no tests of its own while it was a hand-written file inside
 * the crate: what little held it were assertions in `cli/src/html.rs` that the
 * text `EDITOR_ALIASES` appeared somewhere in the page. Every bug named in the
 * comments of `filter.ts` — the ordinal being searched, the tally running into
 * the label, `ds` matching `CHDS` — was found by reading the corpus, and each
 * one has a test here now.
 */
import { beforeEach, describe, expect, it } from 'vitest'
import documentHtml from '../../../cli/src/generated/document.html?raw'
import groupHeadingHtml from '../../../cli/src/generated/group_heading.html?raw'
import manuscriptRowHtml from '../../../cli/src/generated/manuscript_row.html?raw'
import { attachInventoryFilter } from './filter'

interface Row {
  siglum: string
  lang?: string
  corpus?: string
  editor?: string
  year?: string
}

/**
 * The document is built from the very artifacts the crate compiles in.
 *
 * This file used to write the markup out by hand, which made it a second
 * description of a table that already had one — the thing this project keeps
 * paying for. Since the markup moved to `Document.svelte` and its fragments,
 * the artifacts in `cli/src/generated/` are the shape the renderer writes, and
 * a test that says it drives "a document in the shape the renderer writes" can
 * simply use them. A fragment renamed, a cell reordered or a class dropped now
 * shows up here as a failing test rather than as a fixture quietly describing a
 * page that no longer exists.
 */
const artifact = (text: string) => text.replace(/\n+$/, '')

/**
 * `cli/src/html.rs`'s substitution, in the ten characters of it this needs.
 *
 * A callback rather than a replacement string: `$&` and its relatives mean
 * something to `String.replace`, and a siglum is allowed to contain them.
 */
const fill = (template: string, values: Record<string, string>) =>
  template.replace(/@@(\w+)@@/g, (hole: string, name: string) => values[name] ?? hole)

/** The document the renderer writes, with the rows a test asks for. */
function inventory(groups: { label: string; rows: Row[] }[]): void {
  let n = 0
  const body = groups
    .map((group) => {
      const head = fill(artifact(groupHeadingHtml), {
        SPAN: '6',
        LABEL: group.label,
        COUNT: String(group.rows.length),
      })
      const rows = group.rows.map((row) => {
        n += 1
        return fill(artifact(manuscriptRowHtml), {
          NUMBER: String(n),
          TITLE: row.siglum,
          LANG: row.lang ?? 'Hit',
          CORPUS: row.corpus ?? 'HFR',
          EDITOR: row.editor ?? '—',
          YEAR: row.year ?? '2020',
        })
      })
      return [head, ...rows].join('\n')
    })
    .join('\n')

  // The whole page, then the part of it a script sees. The stylesheet and the
  // script are left out — one is not read by anything here and the other is the
  // module under test, reached directly.
  const page = fill(artifact(documentHtml), {
    STYLE: '',
    SCRIPT: '',
    SOURCE: 'test',
    AUTHORS: '—',
    GENERATED: '',
    MANUSCRIPTS: String(n),
    GROUPS: String(groups.length),
    LEGEND: '',
    COLGROUP: '',
    THEAD: '',
    ROWS: body,
  })
  const main = page.slice(page.indexOf('<main>'), page.indexOf('</main>') + '</main>'.length)

  document.body.innerHTML = main
  document.body.className = ''
}

const search = () => document.getElementById('q') as HTMLInputElement
const hint = () => document.getElementById('hint') as HTMLElement
const foldAll = () => document.getElementById('fold-all') as HTMLButtonElement
const headings = () => Array.from(document.querySelectorAll<HTMLElement>('tr.group'))
const items = () =>
  Array.from(document.querySelectorAll<HTMLTableRowElement>('#inv tbody tr:not(.group)'))
const shown = () => items().filter((tr) => !tr.hidden)

function type(query: string): void {
  search().value = query
  search().dispatchEvent(new Event('input'))
}

beforeEach(() => {
  document.body.innerHTML = ''
  document.body.className = ''
})

describe('the controls only appear once the script has run', () => {
  it('marks the document as filtered', () => {
    inventory([{ label: 'CTH 5', rows: [{ siglum: 'KBo 1.1' }] }])
    expect(document.body.classList.contains('filter-on')).toBe(false)
    attachInventoryFilter(document)
    expect(document.body.classList.contains('filter-on')).toBe(true)
  })

  it('does nothing to a document that has no table', () => {
    document.body.innerHTML = '<p>nothing here</p>'
    expect(() => attachInventoryFilter(document)).not.toThrow()
    expect(document.body.classList.contains('filter-on')).toBe(false)
  })
})

describe('search', () => {
  beforeEach(() => {
    inventory([
      { label: 'CTH 1', rows: [{ siglum: 'KBo 1.1', editor: 'DS', year: '2019' }] },
      {
        label: 'CTH 16',
        rows: [
          { siglum: 'KUB 2.1', editor: 'Daniel Schwemer', year: '2021' },
          { siglum: 'KUB 2.2', editor: 'FF', year: '2021' },
        ],
      },
    ])
    attachInventoryFilter(document)
  })

  it('shows everything until something is typed', () => {
    expect(shown()).toHaveLength(3)
    expect(hint().textContent).toBe('')
  })

  it('keeps the rows a query reaches and hides the rest', () => {
    type('kub 2.1')
    expect(shown().map((tr) => tr.cells[1].textContent)).toEqual(['KUB 2.1'])
    expect(hint().textContent).toBe('Match: 1')
  })

  it('says so when nothing matches', () => {
    type('nothing at all')
    expect(shown()).toHaveLength(0)
    expect(hint().textContent).toBe('No matches')
  })

  it('a group whose label matches stands for all of its manuscripts', () => {
    type('cth 16')
    expect(shown()).toHaveLength(2)
    // And the group it is not, whose label only differs by the tally beside
    // it, stays shut: `CTH 1` with one manuscript reads `CTH 11` in its own
    // `textContent`.
    expect(headings().filter((tr) => !tr.hidden)).toHaveLength(1)
  })

  it('does not search the ordinal, which is typography', () => {
    type('3')
    // The third row is numbered 3 and says nothing else about the number.
    expect(shown().map((tr) => tr.cells[1].textContent)).toEqual([])
  })

  it('does not let a query run from one column into the next', () => {
    // `DS` ends the editor cell of row one and `2019` opens the year cell.
    type('ds2019')
    expect(shown()).toHaveLength(0)
  })

  it('finds an editor under either of the spellings the corpus uses', () => {
    type('schwemer')
    expect(shown().map((tr) => tr.cells[1].textContent)).toEqual(['KBo 1.1', 'KUB 2.1'])
    type('ds')
    expect(shown().map((tr) => tr.cells[1].textContent)).toEqual(['KBo 1.1', 'KUB 2.1'])
  })

  it('matches an alias against the editor cell in full, not as a substring', () => {
    inventory([{ label: 'CTH 5', rows: [{ siglum: 'CHDS 1.1', editor: 'AB' }] }])
    attachInventoryFilter(document)
    type('daniel schwemer')
    // The siglum contains `ds`; the person has nothing to do with it.
    expect(shown()).toHaveLength(0)
  })
})

describe('folding', () => {
  beforeEach(() => {
    inventory([
      { label: 'CTH 1', rows: [{ siglum: 'KBo 1.1' }] },
      { label: 'CTH 2', rows: [{ siglum: 'KUB 2.1' }, { siglum: 'KUB 2.2' }] },
    ])
    attachInventoryFilter(document)
  })

  it('folds one group from its heading and opens it again', () => {
    const toggle = headings()[1].querySelector('.group-toggle') as HTMLButtonElement
    toggle.click()
    expect(shown().map((tr) => tr.cells[1].textContent)).toEqual(['KBo 1.1'])
    expect(toggle.getAttribute('aria-expanded')).toBe('false')
    expect(headings()[1].classList.contains('folded')).toBe(true)
    // The heading itself stays: a folded group still says how much is inside.
    expect(headings()[1].hidden).toBe(false)

    toggle.click()
    expect(shown()).toHaveLength(3)
    expect(toggle.getAttribute('aria-expanded')).toBe('true')
  })

  it('folds everything from the toolbar, then opens everything', () => {
    expect(foldAll().textContent).toBe('Collapse fragments')
    foldAll().click()
    expect(shown()).toHaveLength(0)
    expect(foldAll().textContent).toBe('Expand fragments')
    expect(foldAll().getAttribute('aria-expanded')).toBe('false')

    foldAll().click()
    expect(shown()).toHaveLength(3)
    expect(foldAll().textContent).toBe('Collapse fragments')
  })

  it('one press makes a mixture uniform', () => {
    ;(headings()[0].querySelector('.group-toggle') as HTMLButtonElement).click()
    expect(foldAll().textContent).toBe('Collapse fragments')
    foldAll().click()
    expect(shown()).toHaveLength(0)
  })

  it('counts what the query found and what is on screen when they differ', () => {
    foldAll().click()
    type('kub')
    expect(hint().textContent).toBe('Match: 2 · 0 shown')
    foldAll().click()
    expect(hint().textContent).toBe('Match: 2')
  })
})
