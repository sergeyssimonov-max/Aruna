<script lang="ts">
  import { onDestroy } from 'svelte'
  import { open } from '@tauri-apps/plugin-dialog'
  import { openPath } from '@tauri-apps/plugin-opener'
  import { commands, events } from './bindings'
  import type { BuildFailure, BuildProgress, BuildReport, CorpusStats, Stage } from './bindings'

  const CORPUS = 'Thesaurus Linguarum Hethaeorum Digitalis'

  /**
   * Имя каждой стадии по-русски – и записью по объединению, а не словарем со
   * строковым ключом.
   *
   * `Stage` приходит из `bindings.ts` объединением семнадцати литералов, и
   * `Record<Stage, string>` обязывает назвать их все: стадия, добавленная в
   * ядре, роняет `pnpm check` здесь. Ветки «на все остальное» тут нет нарочно –
   * она и есть тот отказ, ради которого стадия объявлена перечислением, а не
   * строкой: она показала бы читателю пустое место вместо забытого имени.
   */
  const STAGES: Record<Stage, string> = {
    'cache-unusable': 'Кеш загрузок не годится – беру архив заново',
    'cached-archive-rejected': 'Архив из кеша не подошел – скачиваю заново',
    'archive-from-cache': 'Архив нашелся в кеше',
    'zenodo-notice': 'Zenodo просит передать',
    'zenodo-unreachable': 'Zenodo не отвечает',
    downloading: 'Скачиваю архив',
    'download-retrying': 'Повторяю загрузку',
    'archive-kept': 'Архив сохранен',
    parsing: 'Разбираю архив',
    'entries-skipped': 'Пропускаю посторонние записи архива',
    indexed: 'Составил указатель документов',
    'reading-headers': 'Читаю заголовки документов',
    'headers-read': 'Заголовки прочитаны',
    writing: 'Записываю документы',
    'checking-package': 'Проверяю, что получилось',
    'checking-published': 'Проверяю опубликованный пакет',
    'previous-package-left': 'Прежний пакет оставлен на месте',
  }

  /**
   * Что окно показывает сейчас.
   *
   * Одно размеченное объединение, а не набор флагов: состояния взаимно
   * исключают друг друга, и пара «идет сборка» с «пакет есть» на экране
   * означала бы, что окно рассказывает о двух разных прогонах сразу. Пути и
   * числа лежат внутри своей ветки по той же причине – в состоянии, где их
   * неоткуда взять, их нельзя и прочитать.
   */
  type Screen =
    | { kind: 'reading' }
    | { kind: 'absent' }
    | { kind: 'present'; stats: CorpusStats; inventory: string }
    | { kind: 'unreadable'; inventory: string | null }
    | { kind: 'building' }
    | { kind: 'built'; report: BuildReport }
    | { kind: 'failed'; failure: BuildFailure }

  let screen: Screen = $state({ kind: 'reading' })
  let trouble: string | null = $state(null)
  let progress: BuildProgress | null = $state(null)
  let manuscripts: number | null = $state(null)
  let groups: number | null = $state(null)
  let note: string | null = $state(null)
  let stopping: boolean = $state(false)

  /**
   * Номера прогонов и архив последнего запуска – намеренно не `$state`.
   *
   * В разметку они не попадают, и реактивными им быть незачем: `$state` здесь
   * объявлял бы зависимость, которой нет, а читателю обещал бы, что от этих
   * трех что-то на экране меняется.
   */
  let job: number | null = null
  let finished: number | null = null
  let archive: string | null = null

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
   * Событие прогресса – своего прогона и только пока он идет.
   *
   * Три отсева подряд, и каждый закрывает свой случай. Первый: состояние уже
   * не «идет сборка» – отчет пришел раньше последнего события, и переписывать
   * им законченный экран нельзя. Второй: событие того прогона, чей отчет уже
   * получен, – такое событие опоздало по определению. Третий: номер прогона
   * защелкивается первым дошедшим событием, и все, что придет с чужим номером,
   * не наше. Своего номера окно заранее знать не может: `JobId` выдается в
   * рабочем потоке, и до первого события его неоткуда взять.
   */
  function advance(event: BuildProgress): void {
    if (screen.kind !== 'building' || event.job === finished) {
      return
    }
    job ??= event.job
    if (event.job !== job) {
      return
    }
    progress = event
    // Числа и предложение защелкиваются, а не показываются на один такт:
    // «нашлось 23 936 документов» сказано один раз, стадией `indexed`, и
    // следующее же событие стерло бы это с экрана, хотя оно не перестало быть
    // правдой.
    if (event.manuscripts !== null) {
      manuscripts = event.manuscripts
    }
    if (event.groups !== null) {
      groups = event.groups
    }
    if (event.note !== null) {
      note = event.note
    }
  }

  /**
   * Подписка на поток прогресса – одна на все время жизни окна.
   *
   * `listen` возвращает отписку через промис, а размонтирование может случиться
   * раньше, чем этот промис исполнится: флаг `gone` – про эту гонку, иначе
   * подписка пережила бы компонент, который ее завел.
   */
  let unlisten: (() => void) | null = null
  let gone: boolean = false

  void events.buildProgress
    .listen((event) => {
      advance(event.payload)
    })
    .then((stop) => {
      if (gone) {
        stop()
      } else {
        unlisten = stop
      }
    })
    .catch((error: unknown) => {
      trouble = String(error)
    })

  onDestroy(() => {
    gone = true
    unlisten?.()
    unlisten = null
  })

  /**
   * Что уже лежит на диске: два вызова подряд, а не один.
   *
   * Где лежит пакет, знает `corpus_location`, и второй догадки об этом в
   * программе быть не должно. `corpus_stats` считает то, что лежит по
   * названному пути, и о папке загрузок ничего не знает.
   *
   * `try` вокруг тегированного результата не лишний: `typedError` в
   * `bindings.ts` пробрасывает настоящий `Error` дальше, а тегирует только то,
   * чем отказала команда. Сорванный мост – это `Error`, и без перехвата окно
   * осталось бы навсегда на «Читаю».
   */
  async function load(): Promise<void> {
    try {
      const located = await commands.corpusLocation()
      if (located.status === 'error') {
        trouble = located.error
        screen = { kind: 'unreadable', inventory: null }
        return
      }
      if (!located.data.package_exists) {
        screen = { kind: 'absent' }
        return
      }
      const counted = await commands.corpusStats(located.data.package)
      if (counted.status === 'error') {
        trouble = counted.error
        // Опись открыть все еще можно, если она на месте: числа не сошлись, а
        // документ – отдельная вещь и от них не зависит.
        screen = {
          kind: 'unreadable',
          inventory: located.data.inventory_exists ? located.data.inventory : null,
        }
        return
      }
      screen = { kind: 'present', stats: counted.data, inventory: located.data.inventory }
    } catch (error: unknown) {
      trouble = String(error)
      screen = { kind: 'unreadable', inventory: null }
    }
  }

  void load()

  /**
   * Запустить сборку и досидеть до ее конца.
   *
   * Все счетчики прогона обнуляются здесь, а не по приходу первого события:
   * между нажатием и первым событием проходит время, и числа прошлого прогона
   * в этот промежуток описывали бы не тот прогон.
   */
  async function build(chosen: string | null): Promise<void> {
    archive = chosen
    job = null
    progress = null
    manuscripts = null
    groups = null
    note = null
    trouble = null
    stopping = false
    screen = { kind: 'building' }
    try {
      const built = await commands.buildCorpus(chosen)
      if (built.status === 'ok') {
        finished = built.data.job
        screen = { kind: 'built', report: built.data }
      } else {
        finished = job
        screen = { kind: 'failed', failure: built.error }
      }
    } catch (error: unknown) {
      // Мост оборвался, а не сборка отказала: у такого исхода нет ни кода из
      // ядра, ни фазы. Повтор ему назначен потому, что чинить тут нечего –
      // единственное осмысленное действие и есть попробовать снова.
      screen = {
        kind: 'failed',
        failure: {
          code: 'broken',
          phase: null,
          message: String(error),
          retryable: true,
          cancelled: false,
        },
      }
    } finally {
      stopping = false
    }
  }

  /** Архив с диска. Отмененный выбор – не событие: окно остается как было. */
  async function pick(): Promise<void> {
    try {
      const chosen = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Архив TLHdig', extensions: ['zip'] }],
      })
      if (chosen === null) {
        return
      }
      await build(chosen)
    } catch (error: unknown) {
      trouble = String(error)
    }
  }

  /**
   * Попросить сборку остановиться – и сказать об этом словом «останавливаю».
   *
   * Подтверждение приходит дважды (§3 контракта): нажатие только подтверждает,
   * что просьбу услышали, а «Остановлено» окно скажет по отказу с `cancelled`,
   * пришедшему из ядра. Между ними может пройти несколько секунд, и это не
   * зависание: запрос метаданных Zenodo и пересчет MD5 архива не прерываются
   * вовсе.
   */
  async function stop(): Promise<void> {
    stopping = true
    try {
      await commands.cancelBuild()
    } catch (error: unknown) {
      // `cancel_build` в Rust возвращает unit и отказать не может, так что
      // сюда попадает только сорванный вызов – а значит, никто ничего не
      // просил, и кнопка обязана вернуться.
      stopping = false
      trouble = String(error)
    }
  }

  /** Опись – документ, и открывает его система, а не окно. */
  async function reveal(path: string): Promise<void> {
    try {
      await openPath(path)
    } catch (error: unknown) {
      trouble = String(error)
    }
  }

  const stage: string = $derived.by(() => (progress === null ? 'Начинаю' : STAGES[progress.stage]))

  /**
   * Доля, когда стадия назвала обе половины, и `null` во всех прочих случаях.
   *
   * Знаменателя может не быть: сервер не обязан объявлять длину загрузки. Ноль
   * знаменателем тоже не считается – доля от него не «сто процентов», а
   * ничего, и полосе в этом случае положено двигаться без деления.
   */
  const fraction: number | null = $derived.by(() => {
    if (progress === null || progress.done === null) {
      return null
    }
    const total = progress.total
    if (total === null || total === 0) {
      return null
    }
    return Math.min(100, Math.round((progress.done / total) * 100))
  })

  /**
   * Заголовок называет состояние, а не программу.
   *
   * До сегодняшнего дня он говорил «готова к работе» безусловно – и в окне,
   * которое только что не смогло ничего прочитать, тоже. Заголовок – первое,
   * что читают, и он обязан быть про то, что происходит.
   */
  const heading: string = $derived.by(() => {
    switch (screen.kind) {
      case 'reading':
        return `Смотрю, что уже собрано`
      case 'absent':
        return `Библиотеки ${CORPUS} здесь еще нет`
      case 'present':
        return `Библиотека ${CORPUS} готова к работе`
      case 'unreadable':
        return `О библиотеке ${CORPUS} сказать нечего`
      case 'building':
        return `Собираю библиотеку ${CORPUS}`
      case 'built':
        return `Библиотека ${CORPUS} собрана`
      case 'failed':
        return screen.failure.cancelled ? 'Остановлено' : 'Собрать не удалось'
    }
  })
</script>

<main>
  <div class="ready">
    <h1>{heading}</h1>

    {#if screen.kind === 'reading'}
      <p class="about">Читаю, что собрано…</p>
    {:else if screen.kind === 'absent'}
      <p class="about">
        Программа возьмет закрепленную запись TLHdig на Zenodo, разложит корпус по папкам в
        загрузках и напишет к нему опись – один файл HTML, который открывается в браузере без сети.
        Это занимает от нескольких секунд до минуты с небольшим.
      </p>
    {:else if screen.kind === 'present'}
      <p class="metrics">
        <span>Manuscripts – <span class="count">{spaced(screen.stats.manuscripts)}</span></span>
        <span>Groups (CTH) – <span class="count">{spaced(screen.stats.groups)}</span></span>
      </p>
      <!--
        Разбивка стоит отдельной строкой и мельче: два итога выше – это ответ
        на вопрос «что собрано», а эти три – на вопрос «как оно устроено»,
        который задают вторым. Строки нет вовсе, когда групп нет: нулями она
        сказала бы о пустом пакете больше, чем о нем известно.
      -->
      {#if screen.stats.spread.largest}
        <p class="spread">
          <span>
            Largest group – <span class="count">{screen.stats.spread.largest.label}</span>
            ({spaced(screen.stats.spread.largest.fragments)})
          </span>
          <span>
            Groups of one – <span class="count">{spaced(screen.stats.spread.singletons)}</span>
          </span>
          <span>
            Without CTH – <span class="count">{spaced(screen.stats.spread.without_cth)}</span>
          </span>
        </p>
      {/if}
      <!--
        Счетчики письма приходят только из манифеста. Когда его нет, строки
        нет: обход каталога считает файлы, а не читает их, и нули на этом месте
        утверждали бы, что аномалий не нашлось, – тогда как их не искали.
      -->
      {#if screen.stats.fonts}
        <p class="spread">
          <span
            >Not in NFC – <span class="count">{spaced(screen.stats.fonts.not_in_nfc)}</span></span
          >
          <span>
            Private use – <span class="count">{spaced(screen.stats.fonts.with_private_use)}</span>
            ({screen.stats.fonts.private_use_points} points)
          </span>
          <span>Anomalies – <span class="count">{spaced(screen.stats.fonts.anomalies)}</span></span>
        </p>
      {/if}
    {:else if screen.kind === 'building'}
      <p class="stage">{stage}</p>
      <!--
        Полоса объявлена `progressbar` и без `aria-valuenow`, когда доли нет:
        по ARIA это и означает «идет, но неизвестно сколько осталось», – ровно
        то, что происходит при загрузке, чью длину сервер не назвал.
      -->
      <div
        class="bar"
        role="progressbar"
        aria-label="Ход сборки"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={fraction ?? undefined}
      >
        {#if fraction === null}
          <div class="bar-waiting"></div>
        {:else}
          <div class="bar-fill" style="width: {fraction}%"></div>
        {/if}
      </div>
      {#if manuscripts !== null}
        <p class="metrics">
          <span>Manuscripts – <span class="count">{spaced(manuscripts)}</span></span>
          {#if groups !== null}
            <span>Groups (CTH) – <span class="count">{spaced(groups)}</span></span>
          {/if}
        </p>
      {/if}
      {#if note}
        <p class="note">{note}</p>
      {/if}
    {:else if screen.kind === 'built'}
      <p class="metrics">
        <span>Documents – <span class="count">{spaced(screen.report.documents)}</span></span>
        <span>Groups (CTH) – <span class="count">{spaced(screen.report.groups)}</span></span>
      </p>
      <!--
        Два младших счетчика показываются только ненулевыми. Ноль здесь – это
        «ничего такого не случилось», и строка о нем занимала бы место, ничего
        не сообщая; ненулевой – редкость, о которой стоит знать.
      -->
      {#if screen.report.disambiguated > 0 || screen.report.stylesheet_dropped > 0}
        <p class="spread">
          {#if screen.report.disambiguated > 0}
            <span>
              Disambiguated – <span class="count">{spaced(screen.report.disambiguated)}</span>
            </span>
          {/if}
          {#if screen.report.stylesheet_dropped > 0}
            <span>
              Stylesheet dropped –
              <span class="count">{spaced(screen.report.stylesheet_dropped)}</span>
            </span>
          {/if}
        </p>
      {/if}
      <p class="where">Пакет – {screen.report.package}</p>
      {#if screen.report.archive}
        <p class="where">Собрано из архива – {screen.report.archive}</p>
      {/if}
    {:else if screen.kind === 'failed'}
      <p class="about">{screen.failure.message}</p>
    {/if}

    <!--
      Мелкая неприятность стоит здесь, под телом состояния, а не вместо него:
      не открылась опись, не прочлись числа – окно сообщает, что смогло, а не
      заменяется сообщением об ошибке целиком.
    -->
    {#if trouble}
      <p class="failure">{trouble}</p>
    {/if}
  </div>

  <div class="foot">
    <div class="controls">
      {#if screen.kind === 'building'}
        <button
          type="button"
          class="control"
          data-testid="primary"
          disabled={stopping}
          onclick={() => void stop()}
        >
          {stopping ? 'Останавливаю…' : 'Отменить'}
        </button>
      {:else if screen.kind === 'present'}
        <!--
          Путь снимается с состояния здесь, а не читается внутри обработчика:
          обработчик – замыкание, и разбор объединения по `kind` внутрь него не
          доходит. `{@const}` стоит там, где ветка уже выбрана.
        -->
        {@const inventory = screen.inventory}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void reveal(inventory)}
        >
          Открыть опись
        </button>
        <button type="button" class="control control-quiet" onclick={() => void build(null)}>
          Пересобрать
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pick()}>
          Взять архив с диска…
        </button>
      {:else if screen.kind === 'built'}
        {@const inventory = screen.report.inventory}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void reveal(inventory)}
        >
          Открыть опись
        </button>
        <button type="button" class="control control-quiet" onclick={() => void build(null)}>
          Пересобрать
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pick()}>
          Взять архив с диска…
        </button>
      {:else if screen.kind === 'failed' && screen.failure.retryable}
        <!--
          Повтор повторяет тот же прогон: тот же архив, если его выбирали, и ту
          же запись Zenodo, если нет. Кнопка, которая после отказа делает не то
          же самое, ответила бы не на тот вопрос.
        -->
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(archive)}
        >
          {screen.failure.cancelled ? 'Собрать' : 'Повторить'}
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pick()}>
          Взять архив с диска…
        </button>
      {:else if screen.kind === 'failed'}
        <!--
          Отказ, который не помечен `retryable`, повторять нечем: тот же прогон
          кончится тем же. Главным действием остается другой архив – это
          единственное, что здесь можно изменить.
        -->
        <button type="button" class="control" data-testid="primary" onclick={() => void pick()}>
          Взять архив с диска…
        </button>
      {:else if screen.kind === 'unreadable'}
        {@const inventory = screen.inventory}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(null)}
        >
          Собрать
        </button>
        {#if inventory !== null}
          <button
            type="button"
            class="control control-quiet"
            onclick={() => void reveal(inventory)}
          >
            Открыть опись
          </button>
        {/if}
        <button type="button" class="control control-quiet" onclick={() => void pick()}>
          Взять архив с диска…
        </button>
      {:else if screen.kind === 'absent'}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(null)}
        >
          Собрать
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pick()}>
          Взять архив с диска…
        </button>
      {/if}
    </div>
    {#if stopping}
      <p class="patience">
        Это может занять несколько секунд: запрос к Zenodo и подсчет контрольной суммы архива
        прервать нельзя, сборка остановится на ближайшем документе.
      </p>
    {/if}
  </div>
</main>
