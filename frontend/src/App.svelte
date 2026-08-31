<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import type { CorpusStats } from './stats'

  type CorpusLocation = {
    downloads: string
    package: string
    inventory: string
    package_exists: boolean
    inventory_exists: boolean
  }

  let stats: CorpusStats | null = $state(null)
  let failure: string | null = $state(null)
  let clicks: number = $state(0)

  /**
   * Два вызова подряд, а не один: где лежит пакет, знает `corpus_location`, и
   * второй догадки об этом в программе быть не должно. `corpus_stats` считает
   * то, что лежит по названному пути, и о папке загрузок ничего не знает.
   *
   * Отказ любого из двух виден на месте метрик – заголовок и кнопка остаются:
   * окно сообщает, что смогло, а не заменяется сообщением об ошибке целиком.
   */
  async function load(): Promise<void> {
    try {
      const where = await invoke<CorpusLocation>('corpus_location')
      stats = await invoke<CorpusStats>('corpus_stats', { path: where.package })
    } catch (error: unknown) {
      failure = String(error)
    }
  }

  void load()

  /**
   * Разряды неразрывным пробелом, вручную.
   *
   * `toLocaleString` дал бы то же самое там, где ICU полон, и другое – там, где
   * он урезан: в jsdom, которым идут модульные тесты, и в WKWebView, которым
   * идет окно, разрядка разная. Число на экране не должно зависеть от того,
   * чем его смотрят.
   */
  function spaced(value: number): string {
    return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ' ')
  }

  /**
   * Кнопка закрывает окно, и счетчик растет раньше выхода.
   *
   * Порядок здесь – контракт со сценарием E2E: под `VITE_E2E` закрывать окно
   * нельзя, иначе проверять станет нечего, но `data-clicks` к этому моменту
   * уже увеличен, и сценарий читает его после первого нажатия.
   */
  async function confirm(): Promise<void> {
    clicks += 1
    if (import.meta.env.VITE_E2E) {
      return
    }
    try {
      await getCurrentWindow().close()
    } catch (error: unknown) {
      failure = String(error)
    }
  }
</script>

<main>
  <div class="ready">
    <h1>Библиотека Thesaurus Linguarum Hethaeorum Digitalis готова к работе</h1>
    {#if failure}
      <p class="failure">{failure}</p>
    {:else if stats}
      <p class="metrics">
        <span>Manuscripts – <span class="count">{spaced(stats.manuscripts)}</span></span>
        <span>Groups (CTH) – <span class="count">{spaced(stats.groups)}</span></span>
      </p>
      <!--
        Разбивка стоит отдельной строкой и мельче: два итога выше – это ответ
        на вопрос «что собрано», а эти три – на вопрос «как оно устроено»,
        который задают вторым. Строки нет вовсе, когда групп нет: нулями она
        сказала бы о пустом пакете больше, чем о нем известно.
      -->
      {#if stats.spread.largest}
        <p class="spread">
          <span>
            Largest group – <span class="count">{stats.spread.largest.label}</span>
            ({spaced(stats.spread.largest.fragments)})
          </span>
          <span>Groups of one – <span class="count">{spaced(stats.spread.singletons)}</span></span>
          <span>Without CTH – <span class="count">{spaced(stats.spread.without_cth)}</span></span>
        </p>
      {/if}
      <!--
        Счетчики письма приходят только из манифеста. Когда его нет, строки
        нет: обход каталога считает файлы, а не читает их, и нули на этом месте
        утверждали бы, что аномалий не нашлось, – тогда как их не искали.
      -->
      {#if stats.fonts}
        <p class="spread">
          <span>Not in NFC – <span class="count">{spaced(stats.fonts.not_in_nfc)}</span></span>
          <span>
            Private use – <span class="count">{spaced(stats.fonts.with_private_use)}</span>
            ({stats.fonts.private_use_points} points)
          </span>
          <span>Anomalies – <span class="count">{spaced(stats.fonts.anomalies)}</span></span>
        </p>
      {/if}
    {:else}
      <p class="failure">Читаю, что собрано…</p>
    {/if}
  </div>
  <button type="button" class="confirm" data-clicks={clicks} onclick={confirm}>Понятно</button>
</main>
