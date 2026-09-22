import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const read = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../src/inventory/${name}`, import.meta.url)), 'utf8')

/** The declarations of the first rule whose selector list is exactly `selector`. */
function rule(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/\s+/g, '\\s+')
  const match = css.match(new RegExp(`(?:^|[}\\n])\\s*${escaped}\\s*\\{([^}]*)\\}`))
  expect(match, `a rule for ${selector}`).not.toBeNull()
  return match![1]
}

/**
 * **Название каталога в заголовке группы: одна строка на экране, целиком по
 * жесту, перенос на узком экране, целиком на бумаге.**
 *
 * Разметку держит `filter.test.ts`, текст и экранирование – тесты `html.rs`.
 * Здесь – что стили говорят то, что обещано: jsdom раскладки не считает, и
 * проверка идет по самим правилам.
 */
describe('название CTH в заголовке группы', () => {
  const screen = read('screen.css')
  const print = read('print.css')
  const narrow = screen.slice(screen.indexOf('@media (max-width: 720px)'))

  it('на обычной ширине – одна строка с многоточием, номер и счет не сжимаются', () => {
    const title = rule(screen, '.group-title')
    expect(title).toMatch(/white-space:\s*nowrap/)
    expect(title).toMatch(/text-overflow:\s*ellipsis/)
    expect(title).toMatch(/overflow:\s*hidden/)
    expect(title).toMatch(/min-width:\s*0/)
    expect(rule(screen, '.group-label,\n.group-count')).toMatch(/flex:\s*none/)
  })

  it('открывается целиком наведением, фокусом и касанием – той же кнопкой', () => {
    const open = rule(screen, '.group-toggle:hover .group-title,\n.group-toggle:focus .group-title')
    expect(open).toMatch(/white-space:\s*normal/)
  })

  it('на узкой ширине уходит на свою строку и переносится', () => {
    const title = rule(narrow, '.group-title')
    expect(title).toMatch(/flex-basis:\s*100%/)
    expect(title).toMatch(/white-space:\s*normal/)
    expect(title).toMatch(/overflow-wrap:\s*anywhere/)
    expect(rule(narrow, '.group-toggle')).toMatch(/flex-wrap:\s*wrap/)
  })

  it('на печати – целиком, без многоточия и без подсказок', () => {
    const inPrint = print.slice(print.indexOf('@media print'))
    const title = rule(inPrint, '.group-title')
    expect(title).toMatch(/white-space:\s*normal/)
    expect(title).toMatch(/text-overflow:\s*clip/)
  })
})
