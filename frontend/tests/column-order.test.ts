import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const HTML_RS = fileURLToPath(new URL('../../cli/src/html.rs', import.meta.url))
const SCREEN_CSS = fileURLToPath(new URL('../src/inventory/screen.css', import.meta.url))

/**
 * **Порядок колонок объявлен в ядре, и стили считают их по счету.**
 *
 * `COLUMNS` в `cli/src/html.rs` – единственное объявление: из него берутся
 * `<col>`, заголовки и порядок ячеек в строке. На узком экране `screen.css`
 * прячет колонку редактора правилом `td:nth-child(5)`, то есть числом, которое
 * повторяет это объявление и ничем с ним не связано. Вставленная перед
 * редактором колонка спрячет чужую, и никто не заметит: разметка останется
 * верной, а спрячется не то.
 *
 * Классы на ячейку не ставятся сознательно. `class="ed"` на каждом из 23 936
 * `<td>` – это 263 КиБ на описи в 5,2 МиБ, пять процентов веса документа ради
 * правила одного медиазапроса. Дешевле держать число и это соответствие.
 */
describe('порядок колонок', () => {
  const rust = readFileSync(HTML_RS, 'utf8')
  const css = readFileSync(SCREEN_CSS, 'utf8')

  /** Классы колонок в том порядке, в каком их объявляет `COLUMNS`. */
  const columns = (() => {
    const block = rust.slice(
      rust.indexOf('const COLUMNS'),
      rust.indexOf('];', rust.indexOf('const COLUMNS')),
    )
    return [...block.matchAll(/class: "([a-z-]+)"/g)].map((m) => m[1])
  })()

  it('читает объявление ядра, а не свою копию', () => {
    expect(columns).toEqual(['c-num', 'c-sig', 'c-lang', 'c-corp', 'c-ed', 'c-year'])
  })

  /**
   * Медиазапрос прячет ровно колонку редактора – ту, чей `<col>` он в том же
   * блоке сужает до нуля. Проверяется совпадение двух способов ее назвать:
   * класса и номера.
   */
  it('на узком экране прячет ту колонку, которую сужает', () => {
    const narrow = css.slice(css.indexOf('@media (max-width: 720px)'))
    const hidden = narrow.match(/td:nth-child\((\d+)\)/)
    expect(hidden, 'правило `td:nth-child(N)` в медиазапросе').not.toBeNull()

    const byNumber = columns[Number(hidden![1]) - 1]
    const byClass = narrow.match(/col\.(c-[a-z]+)\s*\{\s*width:\s*0/)
    expect(byClass, 'правило `col.c-… { width: 0 }` в том же блоке').not.toBeNull()

    expect(byNumber).toBe(byClass![1])
    expect(byNumber).toBe('c-ed')
  })

  /**
   * Заголовок прячется вместе с ячейкой: правило на одну половину колонки
   * оставило бы на узком экране шапку без содержимого.
   */
  it('прячет заголовок тем же номером, что и ячейку', () => {
    const narrow = css.slice(css.indexOf('@media (max-width: 720px)'))
    const cell = narrow.match(/td:nth-child\((\d+)\)/)
    const head = narrow.match(/th:nth-child\((\d+)\)/)
    expect(head, 'правило `th:nth-child(N)`').not.toBeNull()
    expect(head![1]).toBe(cell![1])
  })
})
