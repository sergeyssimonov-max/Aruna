/**
 * Два объявления одной формы данных, сведенные третьим файлом.
 *
 * `specta` и `tauri-specta` в проект не введены, поэтому ответ команды
 * `corpus_stats` описан дважды: типами Rust в `src-tauri/src/lib.rs` и типами
 * TypeScript в `src/stats.ts`. Дублирование само по себе допустимо – его цена
 * известна и невелика, – а вот молчаливое расхождение недопустимо: оно
 * проявится не ошибкой сборки, а пустым местом в окне.
 *
 * Поэтому обе стороны сверяются с одним образцом, `src-tauri/stats-sample.json`:
 *
 *   Rust → `the_wire_shape_is_the_one_the_window_declares` сверяет с ним
 *          сериализацию своих структур;
 *   здесь → тот же файл сверяется с `STATS_SAMPLE`, который объявлен через
 *          `satisfies` от типов окна, так что несовпадение формы литерала и
 *          типа – ошибка `svelte-check`, а несовпадение литерала и файла –
 *          падение этой проверки.
 *
 * Цепь замкнута: Rust ≡ файл, файл ≡ литерал, литерал ≡ типы. Правка одной
 * стороны без остальных роняет одну из двух проверок, и обе входят в набор 6.1.
 *
 * Проверка живет в node-наборе, а не рядом с компонентом: она читает файл
 * репозитория, как это делают `spec-guard`, `readme-links` и
 * `release-version`, а в jsdom `import.meta.url` – не файловый адрес.
 */
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { STATS_SAMPLE } from '../src/stats.ts'

const SAMPLE = fileURLToPath(new URL('../../src-tauri/stats-sample.json', import.meta.url))

describe('форма ответа corpus_stats', () => {
  /**
   * Файл читается с диска, а не импортируется: импорт JSON прошел бы через
   * преобразование сборщика, и сверялось бы то, что получилось у Vite, а не
   * то, что написал Rust.
   */
  it('одна и та же в оболочке и в окне', () => {
    expect(JSON.parse(readFileSync(SAMPLE, 'utf8'))).toEqual(STATS_SAMPLE)
  })

  /**
   * **Пустые места приходят как `null`, а не как отсутствующие поля.**
   *
   * Разница видна только на этом образце, и она существенна для окна: строка
   * про письмо корпуса рисуется по `stats.fonts`, и `undefined` вместо `null`
   * прошел бы ту же проверку `{#if}`, но разошелся бы с типом – а тип здесь
   * единственное, что стоит между окном и полем, которого нет.
   */
  it('называет отсутствующее отсутствующим', () => {
    const recorded = JSON.parse(readFileSync(SAMPLE, 'utf8'))

    expect(recorded.walk.fonts).toBeNull()
    expect(recorded.walk.spread.largest).toBeNull()
    expect(Object.keys(recorded.walk)).toContain('fonts')
  })
})
