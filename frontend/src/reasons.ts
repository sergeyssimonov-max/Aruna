/**
 * Названия причин, по которым документ не является корректным XML.
 *
 * Ключи приходят из манифеста пакета и объявлены в ядре
 * (`cli/src/xml_wellformed.rs`); здесь только то, как их назвать человеку.
 *
 * **Слова выбраны так, чтобы не соврать в две стороны сразу.** Документ не
 * испорчен программой – он таким пришел из корпуса; и он не отвергнут – он
 * лежит в пакете вместе со всеми. Поэтому нигде нет ни «ошибка», ни
 * «отклонен», ни «поврежден»: строки описывают разметку, а не судьбу файла.
 */
const NAMES: Record<string, string> = {
  'unterminated-start-tag': 'tag left open',
  'attribute-not-separated': 'attribute without “=”',
  'attribute-value-unclosed': 'attribute value never closed',
  'attribute-given-twice': 'attribute repeated',
  'element-never-closed': 'element never closed',
  'crossing-elements': 'elements overlap',
  'duplicate-end-tag': 'closing tag repeated',
  'no-such-element': 'closing tag with no element',
  'mismatched-end-tag-name': 'closing tag names another element',
  unclassified: 'not classified',
}

/**
 * Как назвать причину человеку.
 *
 * Незнакомый ключ возвращается как есть, а не подменяется на «неизвестно».
 * Манифест старше окна – обычное дело: ядро может научиться новой причине
 * раньше, чем окно узнает ее имя, и показать ключ честнее, чем скрыть строку
 * или назвать ее пустым словом. Читателю ключ понятен: он же стоит в файле.
 */
export function reasonName(key: string): string {
  return NAMES[key] ?? key
}

/**
 * Известные ключи – для теста, который держит их вместе с ядром.
 */
export const KNOWN_REASONS: readonly string[] = Object.keys(NAMES)
