/**
 * Одно объявление формы данных, и образец, который держит его на настоящих
 * числах.
 *
 * До 02.09.2026 объявлений было два – типы Rust в `src-tauri/src/lib.rs` и типы
 * TypeScript в `src/stats.ts`, – и этот файл существовал, чтобы они не
 * разошлись молча. Дублирования больше нет: `src/bindings.ts` порождается
 * tauri-specta из тех же структур Rust, а его свежесть держит тест оболочки
 * `the_bindings_are_what_these_commands_produce`. Форма теперь не может
 * разойтись – она одна.
 *
 * Проверка осталась, потому что держит она уже не форму, а два обещания,
 * которых порожденный тип не дает:
 *
 *   1. `src-tauri/stats-sample.json` – это то, что Rust действительно
 *      сериализует (`the_wire_shape_is_the_one_the_window_declares` в
 *      оболочке), и здесь тот же файл сверяется с литералом, от которого
 *      отталкиваются компонентные тесты. Фикстура окна и запись Rust – одно и
 *      то же, а не два похожих набора чисел;
 *   2. пустые места приходят как `null`, а не как отсутствующие поля.
 *
 * Цепь: Rust ≡ порожденный тип, Rust ≡ файл, файл ≡ литерал, литерал ≡ тип
 * (через `satisfies` в `src/stats.ts`). Правка одной стороны без остальных
 * роняет одну из проверок, и все они входят в набор 6.1.
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

describe('образец ответа corpus_stats', () => {
  /**
   * Файл читается с диска, а не импортируется: импорт JSON прошел бы через
   * преобразование сборщика, и сверялось бы то, что получилось у Vite, а не
   * то, что написал Rust.
   */
  it('один и тот же в оболочке и в окне', () => {
    expect(JSON.parse(readFileSync(SAMPLE, 'utf8'))).toEqual(STATS_SAMPLE)
  })

  /**
   * **Пустые места приходят как `null`, а не как отсутствующие поля.**
   *
   * Разница видна только на этом образце, и она существенна для окна: строка
   * про письмо корпуса рисуется по `stats.fonts`, и `undefined` вместо `null`
   * прошел бы ту же проверку `{#if}`, но разошелся бы с типом – а тип теперь
   * порожден из Rust, и поле, которого нет, ловится `svelte-check`.
   */
  it('называет отсутствующее отсутствующим', () => {
    const recorded = JSON.parse(readFileSync(SAMPLE, 'utf8'))

    expect(recorded.walk.fonts).toBeNull()
    expect(recorded.walk.spread.largest).toBeNull()
    expect(Object.keys(recorded.walk)).toContain('fonts')
  })
})
