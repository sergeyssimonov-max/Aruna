<script lang="ts">
  import { onDestroy } from 'svelte'
  import { open } from '@tauri-apps/plugin-dialog'
  import { openPath } from '@tauri-apps/plugin-opener'
  import { commands, events } from './bindings'
  import type {
    BuildFailure,
    BuildProgress,
    BuildReport,
    CorpusStats,
    Stage,
    XmlSummary,
  } from './bindings'
  import { reasonName } from './reasons'

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
    | { kind: 'present'; stats: CorpusStats; inventory: string; markup: XmlSummary | null }
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
   * Номера прогонов и папка назначения – намеренно не `$state`.
   *
   * В разметку не попадает ни одно из трех: номера нужны отсеву опоздавших
   * событий, папка – повтору того же прогона. Реактивными им быть незачем:
   * `$state` здесь объявлял бы зависимость, которой нет, а читателю обещал бы,
   * что от этих трех что-то на экране меняется.
   */
  let job: number | null = null
  let finished: number | null = null
  let destination: string | null = null

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
      // Разметка – сведение необязательное, и ее отказ экран не роняет.
      //
      // Секцию `xml` манифест несет с 06.09.2026, и пакет, собранный раньше,
      // ее не несет вовсе: отказ этой команды означает «сказать нечего», а не
      // «что-то не так». Отказ `corpusStats` – другое дело и уводит на
      // `unreadable` в ветке выше: без чисел окно не может сказать о пакете
      // ничего. Здесь же оно теряет один блок и показывает остальное.
      const parsed = await commands.corpusXml(located.data.package)
      screen = {
        kind: 'present',
        stats: counted.data,
        inventory: located.data.inventory,
        markup: parsed.status === 'ok' ? parsed.data : null,
      }
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
    destination = chosen
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

  /** Папка для пакета. Отмененный выбор – не событие: окно остается как было. */
  async function pickFolder(): Promise<void> {
    try {
      const chosen = await open({ multiple: false, directory: true })
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
      <!--
        Окно русское целиком – заголовок, стадии, кнопки, – и эти числа
        оставались в нем последним английским. Оставались наследством описи,
        документа англоязычного, который сам их нигде не показывает: общего
        текста, с которым их пришлось бы держать в согласии, тут нет. Названия
        причин, у которых такое согласие есть, лежат в `src/reasons.ts` и
        взяты из спецификации, §4.13.
      -->
      <p class="metrics">
        <span>Рукописей – <span class="count">{spaced(screen.stats.manuscripts)}</span></span>
        <span>Групп CTH – <span class="count">{spaced(screen.stats.groups)}</span></span>
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
            Самая большая группа – <span class="count">{screen.stats.spread.largest.label}</span>
            ({spaced(screen.stats.spread.largest.fragments)})
          </span>
          <span>
            Групп из одной рукописи –
            <span class="count">{spaced(screen.stats.spread.singletons)}</span>
          </span>
          <span>
            Рукописей без CTH – <span class="count">{spaced(screen.stats.spread.without_cth)}</span>
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
          <span>
            Не в нормальной форме C –
            <span class="count">{spaced(screen.stats.fonts.not_in_nfc)}</span>
          </span>
          <span>
            Со знаками частного использования –
            <span class="count">{spaced(screen.stats.fonts.with_private_use)}</span>
          </span>
          <span>
            Разных таких знаков –
            <span class="count">{spaced(screen.stats.fonts.private_use_points)}</span>
          </span>
          <span>
            Аномалий письма – <span class="count">{spaced(screen.stats.fonts.anomalies)}</span>
          </span>
        </p>
      {/if}
      <!--
        Что разборщик сказал о документах – своей оберткой, а не четырьмя
        соседями `.ready`: тот расставляет детей через 28 пикселей, которыми
        на этом экране отделены друг от друга разные сообщения, а здесь строки
        одного.

        Блока нет вовсе, когда сведений нет: пакет, собранный до 06.09.2026,
        секции `xml` в манифесте не несет, и отказ команды `corpus_xml` – это
        он и есть, а не поломка.
      -->
      {#if screen.markup}
        <div class="markup">
          {#if screen.markup.not_well_formed === 0}
            <p class="spread">
              <span>
                Некорректной разметки нет – проверено
                <span class="count">{spaced(screen.markup.documents)}</span>
              </span>
            </p>
          {:else}
            <p class="spread">
              <span>
                Не является корректным XML –
                <span class="count">{spaced(screen.markup.not_well_formed)}</span>
                из {spaced(screen.markup.documents)}
              </span>
            </p>
            <p class="markup-about">
              Все они лежат в пакете вместе с остальными: это свойство исходных документов, и
              касается оно перевода в PDF, а не хранения.
            </p>
            <!--
              Причина с нулем на экран не попадает: строки читают глазами, и
              десяток подписей, из которых половина ни о чем, отнимает у
              оставшихся ровно то внимание, ради которого разбивка написана.
              В манифесте нули стоят нарочно – это две разные вещи.
            -->
            <p class="spread reasons">
              {#each screen.markup.reasons.filter((r) => r.documents > 0) as r (r.reason)}
                <span
                  >{reasonName(r.reason)} – <span class="count">{spaced(r.documents)}</span></span
                >
              {/each}
            </p>
            <!--
              Имена – под раскрытием и по одному на строку: их две сотни, это
              не сводка, а справка, и открывают ее тогда, когда имя нужно
              найти. `<details>` стоит рядом с абзацами, а не внутри одного из
              них: абзац его содержать не вправе.
            -->
            <details class="names">
              <summary>Имена файлов ({spaced(screen.markup.not_well_formed)})</summary>
              <ul>
                {#each screen.markup.documents_not_well_formed as d (d.file)}
                  <li>
                    <span class="file">{d.file}</span><span class="place"
                      >{reasonName(d.reason)} – строка {d.line}, столбец {d.column}</span
                    >
                  </li>
                {/each}
              </ul>
            </details>
          {/if}
        </div>
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
          <span>Рукописей – <span class="count">{spaced(manuscripts)}</span></span>
          {#if groups !== null}
            <span>Групп CTH – <span class="count">{spaced(groups)}</span></span>
          {/if}
        </p>
      {/if}
      {#if note}
        <p class="note">{note}</p>
      {/if}
    {:else if screen.kind === 'built'}
      <p class="metrics">
        <span>Документов – <span class="count">{spaced(screen.report.documents)}</span></span>
        <span>Групп CTH – <span class="count">{spaced(screen.report.groups)}</span></span>
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
              С повторной сиглой – <span class="count">{spaced(screen.report.disambiguated)}</span>
            </span>
          {/if}
          {#if screen.report.stylesheet_dropped > 0}
            <span>
              Снято инструкций стилей –
              <span class="count">{spaced(screen.report.stylesheet_dropped)}</span>
            </span>
          {/if}
        </p>
      {/if}
      <p class="where">Пакет – {screen.report.package}</p>
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
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
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
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'failed' && screen.failure.retryable}
        <!--
          Повтор повторяет тот же прогон: ту же закрепленную запись Zenodo – и
          ту же папку, если ее выбирали. Кнопка, которая после отказа делает не
          то же самое, ответила бы не на тот вопрос.
        -->
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(destination)}
        >
          {screen.failure.cancelled ? 'Собрать' : 'Повторить'}
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'failed'}
        <!--
          Отказ, который не помечен `retryable`, второй попыткой обычно не
          лечится: тот же прогон кончится тем же. Пока у окна был выбор архива,
          он и стоял здесь главным действием – единственное, что можно было
          изменить. Источник теперь один, и назвать другой окну нечем.

          Кнопка все же стоит, и не для вида: переполненный диск, занятый
          каталог вывода, столкновение имен в нем – это чинится снаружи окна, и
          после починки нужно чем-то начать заново. Второе, что здесь можно
          изменить, не выходя из окна, – папка: рядом стоит ее выбор, и занятый
          каталог вывода обходится другим каталогом. Экран без единого действия
          был бы тупиком. Обещания успеха кнопка не дает, поэтому названа
          «Собрать по умолчанию», а не «Повторить».
        -->
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(null)}
        >
          Собрать по умолчанию
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'unreadable'}
        {@const inventory = screen.inventory}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(null)}
        >
          Собрать по умолчанию
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
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
        </button>
      {:else if screen.kind === 'absent'}
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(null)}
        >
          Собрать по умолчанию
        </button>
        <button type="button" class="control control-quiet" onclick={() => void pickFolder()}>
          Собрать в папку…
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

<!--
  Стили этого блока – здесь, а не в `app.css`.

  Верстка окна лежит в `app.css` целиком, и правило это не отменяется: там
  палитра, шкала размеров и все, что делит между собой несколько экранов.
  Здесь только то, что живет ровно в одной ветке одного состояния и нигде
  больше, – и области видимости Svelte довольно, чтобы этого не объявлять
  глобально. Плоские селекторы и вложенности нет по той же причине, что и
  там: окно живет в WKWebView macOS 13, и вложенный CSS Safari 16 не понимает.
  Цвета и размеры взяты токенами и ступенями `app.css`, своих здесь нет.
-->
<style>
  /* Строки одного сообщения – в своей колонке: промежуток `.ready` в 28
   * пикселей отделяет сообщения друг от друга, а не строки внутри одного. */
  .markup {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  /* Фраза о том, где эти документы лежат, – по мерке прозаических строк
   * экрана: та же ширина и выключка, что у заголовка, и младший размер.
   * Класс свой, а не `.about`: тот объявлен глобально и принадлежит строкам,
   * которые сменяют друг друга по состояниям, – эта стоит вместе с ними. */
  .markup-about {
    max-width: 480px;
    margin: 0;
    text-align: center;
    font-size: 13px;
  }

  /* Разбивка переносится по строкам, в отличие от соседних `.spread`: причин
   * бывает десяток, а ширина окна не меняется. Промежуток между строками
   * меньше, чем между подписями в строке, – иначе перенос читался бы как
   * начало другого списка. */
  .reasons {
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px 32px;
  }

  /* Ширина названа явно и равна ширине заголовка: без нее `align-items:
   * center` сжал бы раскрытый список по самой длинной строке, и он ездил бы
   * по горизонтали от того, какие имена попались. */
  .names {
    width: 480px;
    font-size: 13px;
  }

  .names summary {
    cursor: pointer;
  }

  /* Список прокручивается сам. Двести с лишним позиций вытолкнули бы низ окна
   * за край: окно 800×600 и размер мышью не меняет, а прокрутки целиком у
   * него нет. */
  .names ul {
    max-height: 96px;
    overflow-y: auto;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }

  .names li {
    padding: 3px 0;
  }

  /* Имя ломается где придется, как и путь пакета в `.where`: это путь внутри
   * пакета, и пробелов в нем не бывает. */
  .names .file {
    color: var(--text-h);
    overflow-wrap: anywhere;
  }

  /* Причина и место – в той же строке, мельче имени и цветом подписи: имя
   * здесь ищут, а это читают, найдя. Строка одна, а не две: раскрытый список
   * ограничен по высоте, и вторая строка на документ вдвое сократила бы то,
   * что видно без прокрутки. */
  .names .place {
    margin-left: 8px;
    font-size: 12px;
    color: var(--text);
  }
</style>
