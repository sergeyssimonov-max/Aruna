/**
 * Образцы того, что команда `corpus_stats` присылает в окно.
 *
 * **Тип здесь больше не объявляется.** До 02.09.2026 он был написан от руки и
 * дважды – здесь и в `src-tauri/src/lib.rs`, – потому что specta и tauri-specta
 * в проект введены не были, а расхождение двух ручных объявлений закрывалось
 * файлом образцов и парой проверок вокруг него. Теперь тип порождается из Rust
 * в `bindings.ts`, и звено «объявлено вручную дважды» из цепи выпало.
 *
 * Цепь стала короче и строже:
 *
 *   Rust → `bindings.ts` – порождается из объявления команд, свежесть держит
 *          `the_bindings_are_what_these_commands_produce` в оболочке;
 *   Rust → `src-tauri/stats-sample.json` – сериализация тех же структур,
 *          сверяется `the_wire_shape_is_the_one_the_window_declares`;
 *   файл → `STATS_SAMPLE` ниже, сверяется `tests/ipc-shape.test.ts`;
 *   `STATS_SAMPLE` → порожденный тип, через `satisfies` – значит несовпадение
 *          формы становится ошибкой `svelte-check`, а не неожиданностью в окне.
 *
 * Образец остался, хотя типы его больше не подпирают: он показывает, какого
 * порядка числа тут бывают, и – второй его половиной – что пустые места
 * приходят как `null`, а не как отсутствующие поля. Придуманный из головы
 * образец сверял бы форму с самим собой; этот взят из манифеста настоящего
 * пакета.
 */
import type { CorpusStats } from './bindings.ts'

type StatsSamples = {
  /** Манифест на месте: он отвечает и о разбивке, и о письме. */
  readonly manifest: CorpusStats
  /** Манифеста нет: два итога и разбивка есть, о письме сказать нечего. */
  readonly walk: CorpusStats
}

/**
 * Те же образцы, что записаны в `src-tauri/stats-sample.json`.
 *
 * Числа настоящие – из манифеста текущего пакета: 23 936 фрагментов в 663
 * группах, самая большая CTH 832 с 4 480, 116 групп из одного фрагмента, ни
 * одного фрагмента без CTH.
 */
export const STATS_SAMPLE = {
  manifest: {
    manuscripts: 23936,
    groups: 663,
    source: 'manifest',
    spread: {
      largest: { label: 'CTH 832', fragments: 4480 },
      singletons: 116,
      without_cth: 0,
    },
    fonts: {
      not_in_nfc: 78,
      with_private_use: 1269,
      private_use_points: 7,
      anomalies: 0,
    },
  },
  walk: {
    manuscripts: 0,
    groups: 0,
    source: 'walk',
    spread: { largest: null, singletons: 0, without_cth: 0 },
    fonts: null,
  },
} as const satisfies StatsSamples
