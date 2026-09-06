import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { KNOWN_REASONS, reasonName } from '../src/reasons.js'

const CORE = fileURLToPath(new URL('../../cli/src/xml_wellformed.rs', import.meta.url))

/**
 * **Причины объявлены в ядре, и окно не вправе знать другой список.**
 *
 * Список в двух местах расходится молча: ядро научится новой причине, манифест
 * начнет ее писать, а окно покажет сырой ключ и никто не заметит. Тест читает
 * ключи прямо из `Reason::key` – не копию, а объявление.
 */
describe('названия причин', () => {
  const keys = [...readFileSync(CORE, 'utf8').matchAll(/=> "([a-z-]+)",/g)].map((m) => m[1])

  it('перечисляет ровно те ключи, что объявляет ядро', () => {
    expect(keys.length).toBeGreaterThan(0)
    expect([...KNOWN_REASONS].sort()).toEqual([...keys].sort())
  })

  it('каждому ключу дает имя, отличное от самого ключа', () => {
    for (const key of keys) {
      expect(reasonName(key), key).not.toBe(key)
    }
  })

  /**
   * Ни одно из названий не приписывает документу того, чего с ним не делали.
   *
   * Договор задания дословно: документ не «испорчен нами» и не «отвергнут», он
   * не является корректным XML, и в пакете он есть. Слова ниже это нарушают, и
   * тест держит границу, потому что подобрать их заново легко, а заметить
   * подмену на экране – нет.
   */
  it('не называет документ испорченным или отвергнутым', () => {
    const forbidden = /broken|corrupt|invalid|rejected|discarded|excluded|bad|error|failed/i
    for (const key of keys) {
      expect(reasonName(key), key).not.toMatch(forbidden)
    }
  })

  it('незнакомый ключ показывает как есть, а не прячет', () => {
    expect(reasonName('something-new')).toBe('something-new')
  })
})
