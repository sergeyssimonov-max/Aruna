/**
 * Что команда `corpus_stats` присылает в окно.
 *
 * Объявлено вручную и дважды – здесь и в `src-tauri/src/lib.rs`, – потому что
 * specta и tauri-specta в проект не введены. Ручное дублирование без проверки
 * живет до первой правки Rust, поэтому расхождение закрыто с двух сторон одним
 * файлом образцов, `src-tauri/stats-sample.json`:
 *
 *   Rust → `the_wire_shape_is_the_one_the_window_declares` сверяет
 *          сериализацию с этим файлом;
 *   TypeScript → `App.test.ts` сверяет тот же файл с `STATS_SAMPLE` ниже, а
 *          `STATS_SAMPLE` объявлен через `satisfies` от типов этого модуля –
 *          значит несовпадение формы становится ошибкой `svelte-check`, а не
 *          неожиданностью в окне.
 *
 * Цепь замкнута: Rust ≡ файл, файл ≡ литерал, литерал ≡ типы. Порвать ее одной
 * правкой нельзя – порвется одна из двух проверок.
 */

/** Группа и ее размер. */
type GroupSize = {
  label: string
  fragments: number
}

/**
 * Как фрагменты разложены по группам CTH.
 *
 * `largest` – `null` там, где групп нет вовсе: ноль фрагментов в
 * несуществующей группе не утверждение, а отсутствие утверждения.
 */
type Spread = {
  largest: GroupSize | null
  singletons: number
  without_cth: number
}

/**
 * Что манифест насчитал о письме корпуса при разборе.
 *
 * `null` целиком, когда числа получены обходом каталога: обход считает файлы и
 * о том, как написан их текст, знать не может. Нули вместо `null` сказали бы,
 * что аномалий нет, – а сказать нечего.
 */
type Fonts = {
  not_in_nfc: number
  with_private_use: number
  private_use_points: number
  anomalies: number
}

export type CorpusStats = {
  manuscripts: number
  groups: number
  source: 'manifest' | 'walk'
  spread: Spread
  fonts: Fonts | null
}

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
 * одного фрагмента без CTH. Образец, придуманный из головы, сверял бы форму с
 * самим собой; этот вдобавок показывает, какого порядка числа тут бывают.
 *
 * Второй образец существует ради пустых мест: только он закрепляет, что
 * `largest` и `fonts` приходят как `null`, а не как отсутствующие поля.
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
