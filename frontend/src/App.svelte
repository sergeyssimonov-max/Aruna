<script lang="ts">
  import { onDestroy } from 'svelte'
  import { open } from '@tauri-apps/plugin-dialog'
  import { commands, events } from './bindings'
  import type {
    BuildFailure,
    BuildProgress,
    BuildReport,
    CorpusStats,
    Stage,
    XmlSummary,
  } from './bindings'

  const CORPUS = 'Thesaurus Linguarum Hethaeorum Digitalis'

  /**
   * Имя каждой стадии по-русски – и записью по объединению, а не словарем со
   * строковым ключом.
   *
   * `Stage` приходит из `bindings.ts` объединением девятнадцати литералов, и
   * `Record<Stage, string>` обязывает назвать их все: стадия, добавленная в
   * ядре, роняет `pnpm check` здесь. Ветки «на все остальное» тут нет нарочно –
   * она и есть тот отказ, ради которого стадия объявлена перечислением, а не
   * строкой: она показала бы читателю пустое место вместо забытого имени.
   */
  const STAGES: Record<Stage, string> = {
    'cache-unusable': 'Кеш загрузок не годится – беру архив заново',
    'cached-archive-rejected': 'Архив из кеша не подошел – скачиваю заново',
    'archive-from-cache': 'Архив нашелся в кеше',
    'zenodo-notice': 'Hethitologie-Portal Mainz просит передать',
    'zenodo-unreachable': 'Hethitologie-Portal Mainz не отвечает',
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
    'waiting-for-publication': 'Жду, пока другой запуск закончит публикацию в эту папку',
    'publishing-without-lock':
      'Этот диск не держит блокировку публикации – публикую без защиты от одновременного запуска',
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
    | { kind: 'present'; stats: CorpusStats; inventory: string | null; markup: XmlSummary | null }
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
   * Номера прогонов – намеренно не `$state`.
   *
   * В разметку не попадает ни один: они нужны отсеву опоздавших событий.
   * Реактивными им быть незачем: `$state` здесь объявлял бы зависимость,
   * которой нет, а читателю обещал бы, что от них что-то на экране меняется.
   */
  let job: number | null = null
  let finished: number | null = null

  /**
   * Папка назначения – `$state`, и это изменение 19.09.2026.
   *
   * До появления ветки отмены папка в разметку не попадала и стояла рядом с
   * номерами прогонов, обычной переменной. Теперь от нее зависит подпись
   * главной кнопки на остановленном прогоне – «Собрать» против «Собрать по
   * умолчанию», – то есть на экране от нее меняется видимое, и зависимость
   * должна быть объявлена.
   */
  let destination: string | null = $state(null)

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
      // Опись отдельным флагом, как и в ветке `unreadable` выше: два флага
      // бывают разными – каталог пакета на месте, описи в нем уже нет, – и
      // кнопка над отсутствующим файлом могла кончиться только отказом.
      screen = {
        kind: 'present',
        stats: counted.data,
        inventory: located.data.inventory_exists ? located.data.inventory : null,
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
      //
      // Номер прогона снимается и здесь: иначе опоздавшее событие прошлого
      // прогона защелкивается заново, и все события следующего прогона окно
      // отбрасывает как чужие.
      finished = job
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

  /**
   * Опись – документ, и открывает его система, а не окно.
   *
   * Просит об этом ядро, а не плагин из окна: до 07.09.2026 здесь стоял
   * `openPath`, и он отказывал всегда. Разрешение включает команду, но области
   * путей ей не дает, а наполнить область было нечем – опись лежит там, куда
   * выбрали собирать. Граница переехала в `open_inventory`, где она уже: один
   * файл с именем, объявленным ядром, где бы он ни лежал.
   *
   * Удачное открытие `trouble` не гасит: там может стоять сообщение о другом –
   * например, о числах, которые не сошлись при чтении папки, – и оно остается
   * верным.
   */
  async function reveal(path: string): Promise<void> {
    try {
      const opened = await commands.openInventory(path)
      if (opened.status === 'error') {
        trouble = opened.error
      }
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

  /**
   * Пояснение к остановленному прогону – его составляет окно, а не ядро.
   *
   * Довод записан ниже, над `FAILED`, и здесь не повторяется. Своя таблица
   * отмене нужна потому, что различает ее не код, который у всякой отмены
   * один, а фаза: после загрузки на диске не изменилось ничего, после записи
   * убрано незаконченное. Фаза берется с провода, так что собственного знания
   * о том, что и в каком порядке делает ядро, у окна не появляется.
   *
   * Ветка «на все остальное» здесь нужна, и это не спор со `STAGES` выше:
   * `Phase::code` приходит строкой, а не объединением литералов, и потребовать
   * от типа полноты тут нечем. Пять кодов ядра названы поименно, а шестой,
   * если он когда-нибудь появится, получит общую фразу вместо пустого места.
   */
  const CANCELLED_ANYWHERE = 'Остановлено. Незаконченное убрано, прежний пакет остался на месте.'

  const CANCELLED: Record<string, string | undefined> = {
    obtaining: 'Остановлено во время загрузки архива. На диске ничего не изменилось.',
    parsing: 'Остановлено во время чтения архива. На диске ничего не изменилось.',
    exporting:
      'Остановлено во время записи пакета. Незаконченное убрано, прежний пакет остался на месте.',
    validating:
      'Остановлено во время проверки записанного. Незаконченное убрано, прежний пакет остался на месте.',
    publishing: 'Остановлено перед самой заменой пакета. Прежний пакет остался на месте.',
  }

  /**
   * Русская фраза на каждый код отказа – ее составляет окно, а не ядро.
   *
   * У ядра один `message` на оба входа, и он английский: консольный вход
   * печатает его рядом с английскими же причинами, и читает его тот, кто
   * собирает программу из исходников. Перевести эту фразу в ядре значило бы
   * испортить консольный вывод ради окна. Окно русское целиком, и человек за
   * ним не видит ничего, кроме окна, поэтому фразы для него собраны здесь.
   *
   * Источник назван составной формой – «Hethitologie-Portal Mainz (запись на
   * Zenodo)». Программа обращается к zenodo.org, и это имя человек увидит сам,
   * стоит ему открыть журнал или настройки сети, а издатель корпуса –
   * Hethitologie-Portal Mainz, и по нему корпус ищут. Одного имени мало:
   * первое не говорит, чье это, второе не говорит, куда стучалась программа.
   *
   * У `collision` и `archive_duplicate` под этой фразой печатается вторая
   * строка с `message` ядра, и только у них: там фраза ядра несет данные,
   * которых на экране больше нет нигде – имена двух столкнувшихся документов и
   * имя записи архива. Без них человеку нечего искать в корпусе, а сама фраза
   * ядра про эти два кода ничего сверх имен и не сообщает.
   *
   * Запасной путь на неизвестный код оставлен намеренно: новый код ядра,
   * которого окно еще не знает, покажет английскую фразу ядра, и это будет
   * видно, а не станет пустым местом.
   *
   * **Это один из двух наборов русских фраз, и разделение намеренное.** Второй
   * живет в `cli/src/main.rs`, в функции `advice`, и говорит о тех же отказах
   * то же самое. Граница записана там же полностью, здесь коротко: консоль
   * читает тот, кто собирает из исходников, поэтому ей можно пути, имена
   * констант и ссылку на страницу выпусков, и источник она зовет `Zenodo`.
   * Окну нельзя ничего из этого: у его читателя есть только приложение, и
   * источник для него – «Hethitologie-Portal Mainz (запись на Zenodo)», решение
   * владельца от 19.09.2026 для окна и только для окна. Противоречить друг
   * другу о том, что случилось, эти два набора не вправе, а разные слова для разных
   * читателей – в порядке вещей.
   */
  const FAILED: Record<string, string | undefined> = {
    // Что помешало получить архив.
    network:
      'Не удалось связаться с Hethitologie-Portal Mainz (запись на Zenodo). Проверьте подключение к сети и повторите.',
    server_busy:
      'Hethitologie-Portal Mainz (запись на Zenodo) сейчас не успевает отвечать. Это временно: попробуйте позже.',
    http: 'Hethitologie-Portal Mainz (запись на Zenodo) отказался отдать файл. Чаще всего это значит, что запись перевыпущена: поможет свежая версия Aruna, а не повтор.',
    truncated: 'Загрузка оборвалась на полпути: пришло меньше, чем обещал сервер. Повторите.',
    oversized:
      'Ответ оказался длиннее, чем сервер сам объявил. Обычно это не сам источник, а что-то по дороге: портал Wi-Fi, рабочий прокси или подмена ответа.',
    checksum:
      'Архив скачался целиком, но его контрольная сумма не совпала с ожидаемой. Повтор не поможет – сумма не изменится, нужна свежая версия Aruna.',

    // Что нашлось в самом архиве.
    archive_unreadable: 'Архив не читается: он поврежден.',
    archive_empty: 'В архиве нет ни одного документа XML.',
    archive_too_many_entries:
      'В архиве больше записей, чем программа готова прочитать. В корпусе TLHdig их около 24 500, значит, это не он. Ничего не распаковано.',
    document_too_large:
      'Один документ в архиве больше допустимого предела и прочитан не был. Обычно так выглядит поврежденный архив. Пакет не собран, память не израсходована.',
    archive_duplicate:
      'В архиве два документа с одним и тем же именем. Это расхождение в исходных данных, а не сбой сборки.',

    // Что помешало записать пакет.
    collision:
      'Два документа претендуют на одно место в пакете. Это расхождение в исходных данных, а не сбой сборки.',
    distorted:
      'Документ изменился при обработке сверх допустимого, и сборка остановлена. Ничего не опубликовано: это защита содержимого, а не сбой записи.',
    package_too_large:
      'Пакет вырос больше допустимого предела, и сборка остановлена. Прежний пакет не тронут: все писалось во временную папку, и она удалена.',
    package_invalid:
      'Пакет собран, но не сошелся со своей же моделью, поэтому не опубликован. Это ошибка программы, а не ваших данных.',
    package_incomplete:
      'Записано меньше документов, чем было размечено, поэтому пакет не опубликован. Это ошибка программы, а не ваших данных.',

    // Папка назначения и подмена пакета.
    no_output_directory:
      'Не удалось определить папку загрузок. Выберите папку сами – кнопка «Собрать в папку…» рядом.',
    destination_not_ours:
      'В выбранной папке лежит что-то, чего программа не создавала. Она ничего не удалила: перенесите эту папку в сторону или выберите другую.',
    publish_busy:
      'В ту же папку сейчас пишет другой запуск Aruna. Дождитесь его окончания и повторите.',
    output_locked:
      'Прежнюю опись не удалось заменить: файл кем-то открыт. Новая опись готова и лежит рядом. Закройте программу, которая держит старый файл – обычно это браузер – и повторите.',
    io: 'Диску не удалось отдать или принять файл. Чаще всего это нехватка места или нет прав на папку.',

    // Шрифты, с которыми приложение установлено.
    font_missing:
      'Приложение установлено не полностью: файла шрифта нет на месте. Переустановите Aruna из образа – шрифты приезжают с приложением и не скачиваются.',
    font_altered:
      'Файл шрифта не совпадает с записанным. Подставлять вместо него другой программа не станет: в PDF это дало бы не тот знак.',

    // Свой код окна, а не ядра.
    broken: 'Связь с ядром программы оборвалась. Чинить тут нечего – повторите.',
  }

  /**
   * Два кода, у которых под основной фразой стоит вторая строка. Набор, а не
   * проверка на два равенства в разметке: почему этих кодов ровно два,
   * объяснено над `FAILED`, и место объяснения должно быть одно.
   */
  const DETAILED: ReadonlySet<string> = new Set(['collision', 'archive_duplicate'])

  /**
   * Чистая функция, а не `$derived`: пояснение читает одна ветка разметки, и
   * `kind` там уже сужен до `failed`. Производная величина потребовала бы
   * часового на прочие состояния и пустой строки, которой на экране не бывает.
   */
  function explain(failure: BuildFailure): string {
    if (!failure.cancelled) {
      return FAILED[failure.code] ?? failure.message
    }
    return (failure.phase === null ? undefined : CANCELLED[failure.phase]) ?? CANCELLED_ANYWHERE
  }
</script>

<main>
  <div class="ready">
    <h1>{heading}</h1>

    {#if screen.kind === 'reading'}
      <p class="about">Читаю, что собрано…</p>
    {:else if screen.kind === 'absent'}
      <p class="about">
        Программа возьмет TLHdig с Hethitologie-Portal Mainz (запись на Zenodo), разложит корпус по
        папкам в загрузках и создаст опись – один файл HTML, который открывается в браузере без
        сети. Это занимает от нескольких секунд до минуты с небольшим.
      </p>
    {:else if screen.kind === 'present'}
      <!--
        Окно русское целиком – заголовок, стадии, кнопки, – и эти числа
        оставались в нем последним английским. Оставались наследством описи,
        документа англоязычного, который сам их нигде не показывает: общего
        текста, с которым их пришлось бы держать в согласии, тут нет.
      -->
      <p class="metrics">
        <span>Рукописей – <span class="count">{spaced(screen.stats.manuscripts)}</span></span>
        <span>Групп CTH – <span class="count">{spaced(screen.stats.groups)}</span></span>
      </p>
      <!--
        Разбивка стоит отдельной строкой: два итога выше – это ответ на вопрос
        «что собрано», а эти две – на вопрос «как оно устроено», который задают
        вторым. Строки нет вовсе, когда групп нет: нулями она сказала бы о
        пустом пакете больше, чем о нем известно.
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
        </p>
      {/if}
      <!--
        Что разборщик сказал о документах – в своей обертке: `.ready`
        расставляет детей через 28 пикселей, которыми отделены друг от друга
        разные сообщения, а здесь строки одного – счетчик и, когда об этих
        документах есть что сказать, абзацы под ним. Класс `.markup` отделяет
        разметку от итогов выше, и на него опираются тесты. Веток у счетчика
        три – «прочитаны все», «столько-то не могут быть преобразованы в PDF»
        и, для пакета без счета модели, «программа не прочитала столько-то».

        Блока нет вовсе, когда сведений нет: пакет, собранный до 06.09.2026,
        секции `xml` в манифесте не несет, и отказ команды `corpus_xml` – это
        он и есть, а не поломка.

        Крупным числом стоит `objected_to` – множество, которое не принимает
        модель документа (`cli/src/document.rs`), и последствие «не попадут в
        PDF» есть только у него. 206 – классификация разборщика, и до
        13.09.2026 это число стояло здесь на его месте, с последствием,
        принадлежащим 223.

        Третья ветка – пакет, собранный 06.09–10.09.2026: секция `xml` в нем
        есть, а счета модели нет, и `objected_to` приходит нулем. 206 там
        показано под своим собственным вопросом – что программа не прочитала,
        – и без последствия для PDF: приписать его этому числу значило бы
        повторить ту самую подмену.
      -->
      {#if screen.markup}
        <div class="markup">
          {#if screen.markup.objected_to > 0}
            <p class="spread">
              <span>
                Не могут быть преобразованы в PDF –
                <span class="count">{spaced(screen.markup.objected_to)}</span>
                из {spaced(screen.markup.documents)}
              </span>
            </p>
            <p class="markup-about">
              Программа их не читает: разметка каждого нарушает правила XML или правила пространств
              имен.
            </p>
            <p class="markup-about">
              Все они лежат в пакете вместе с остальными и открываются как обычно – такая разметка
              мешает переводу в PDF, а не хранению.
            </p>
          {:else if screen.markup.unread === 0}
            <p class="spread">
              <span>
                Прочитаны все – <span class="count">{spaced(screen.markup.documents)}</span>
              </span>
            </p>
          {:else}
            <p class="spread">
              <span>
                Программа не прочитала –
                <span class="count">{spaced(screen.markup.unread)}</span>
                из {spaced(screen.markup.documents)}
              </span>
            </p>
            <p class="markup-about">
              Разметка нарушена внутри тела документа – не сходятся теги, пропущен знак «=» в
              атрибуте, значение атрибута не закрыто до конца файла. Все они лежат в пакете вместе с
              остальными и открываются как обычно.
            </p>
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
        Двумя числами экран и кончается – решением владельца 08.09.2026.
        «Переименовано из-за совпадения» и «С лишней ссылкой на оформление»
        стояли здесь семнадцать часов: 07.09 подписи переписали для читателя и сняли
        правило показывать их только ненулевыми, а на следующий день сняли и
        сами строки. Экран отвечает на вопрос «что получилось», и на него
        отвечают документы и группы; починка имен и снятая ссылка на
        оформление – это то, как шла сборка, а не то, что из нее вышло.

        Числа не потеряны, а только не показаны: `disambiguated` и
        `stylesheet_dropped` идут по проводу по-прежнему и лежат в отчете
        ядра. Ровно так же сняты счетчики письма 06.09.2026 – с экрана, но не
        с провода, и последняя проверка их обоих – в `App.test.ts`.
      -->
      <p class="where">Пакет – {screen.report.package}</p>
    {:else if screen.kind === 'failed'}
      <p class="about">{explain(screen.failure)}</p>
      <!--
        Вторая строка – фраза ядра целиком, и стоит она у двух кодов из
        `DETAILED`. Класс `where` взят потому, что он уже набран младшим
        размером и ломает длинные слова: там стоит имя документа, а окно не
        меняет ширину и не прокручивается.
      -->
      {#if DETAILED.has(screen.failure.code)}
        <p class="where">{screen.failure.message}</p>
      {/if}
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
        <!--
          Описи может не быть и при целом пакете, и тогда предлагать ее незачем:
          главным действием становится пересборка – единственное, чем окно может
          эту опись вернуть.
        -->
        {#if inventory !== null}
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
        {:else}
          <button
            type="button"
            class="control"
            data-testid="primary"
            onclick={() => void build(null)}
          >
            Пересобрать
          </button>
        {/if}
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
      {:else if screen.kind === 'failed' && screen.failure.cancelled}
        <!--
          У отмены ветка своя, и вот почему она нужна. Ядро метит любую отмену
          `retryable: false`, и это верно: `retryable` – совет о неисправности,
          а остановил прогон человек, неисправности не было. Окно же читало
          один этот флаг, и отмененный прогон уходил в ветку неповторимого
          отказа, где главное действие – `build(null)`. Папка, которую человек
          только что выбрал, забывалась, а тернарник `cancelled ? …` в соседней
          ветке был мертв: провод такой формы не выдает.

          Повтор повторяет тот же прогон – ту же запись и ту же папку, если ее
          выбирали. Подпись следует за поведением: «по умолчанию» стоит там и
          только там, где зовут `build(null)`.
        -->
        <button
          type="button"
          class="control"
          data-testid="primary"
          onclick={() => void build(destination)}
        >
          {destination === null ? 'Собрать по умолчанию' : 'Собрать'}
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
          Повторить
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
        Это может занять несколько секунд: запрос к Hethitologie-Portal Mainz и подсчет контрольной
        суммы архива прервать нельзя, сборка остановится на ближайшем документе.
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
  /* Строки одного сообщения – в своей колонке с промежутком: 28 пикселей
   * `.ready` отделяют сообщения друг от друга, а не строки внутри одного.
   *
   * Своих отступов у пяти чисел нет, и это решение, а не недосмотр. `.ready`
   * расставляет их через 28 пикселей, и на опустевшем экране они читаются
   * группой сверху вниз сами: два итога, под ними разбивка, под ней разметка.
   * Сдвинуть их плотнее отсюда нечем и не нужно: `.metrics` и `.spread`
   * объявлены в `app.css`, а `.metrics` носят заодно строки прогона и отчета,
   * и правило в этом файле переставило бы заодно и их – ради экрана, о
   * котором его не просили. */
  .markup {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  /* Фраза о том, где эти документы лежат, – по мерке прозаических строк
   * экрана: та же ширина и выключка, что у заголовка. Класс свой, а не
   * `.about`: тот объявлен глобально и принадлежит строкам, которые сменяют
   * друг друга по состояниям, – эта же стоит рядом с числами. */
  .markup-about {
    max-width: 480px;
    margin: 0;
    text-align: center;
  }
</style>
