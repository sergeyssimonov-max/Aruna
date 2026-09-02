import { cleanup, fireEvent, render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { tick } from 'svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App.svelte'
import { STATS_SAMPLE } from './stats'
import type { BuildFailure, BuildProgress, BuildReport, CorpusStats } from './bindings'

/**
 * Мост в Tauri – один модуль, и заглушка тоже одна.
 *
 * `bindings.ts` порожден tauri-specta и с этого дня единственная поверхность
 * Tauri, которую окно трогает напрямую: подменять `@tauri-apps/api/core` и
 * `@tauri-apps/api/event` порознь значило бы описывать в тесте еще и рантайм
 * specta – два тегированных исхода, `makeEvent` и приведение полезной нагрузки,
 * – ничего из чего окно не выбирает.
 *
 * Два плагина остаются отдельно: их окно зовет само, минуя `bindings.ts`, и в
 * этом весь смысл их подмены – проверить, что зовет именно их и именно с тем
 * путем.
 *
 * `vi.hoisted` здесь не украшение: фабрику `vi.mock` поднимают выше импортов, и
 * обычная переменная в этот момент еще не создана.
 */
const { corpusLocation, corpusStats, buildCorpus, cancelBuild, listen, unlisten, open, openPath } =
  vi.hoisted(() => ({
    corpusLocation: vi.fn(),
    corpusStats: vi.fn(),
    buildCorpus: vi.fn(),
    cancelBuild: vi.fn(),
    listen: vi.fn(),
    unlisten: vi.fn(),
    open: vi.fn(),
    openPath: vi.fn(),
  }))

vi.mock('./bindings', () => ({
  commands: { corpusLocation, corpusStats, buildCorpus, cancelBuild },
  events: { buildProgress: { listen } },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open }))
vi.mock('@tauri-apps/plugin-opener', () => ({ openPath }))

const DOWNLOADS = '/Users/reader/Downloads'
const PACKAGE = `${DOWNLOADS}/TLHdig_Beta_0.3`
const INVENTORY = `${PACKAGE}/TLHdig_Beta_0.3.html`
const ARCHIVE = `${DOWNLOADS}/tlhdig-0.3.zip`

/** Событие прогресса, отправленное с той стороны провода. */
let emit: (payload: BuildProgress) => void = () => {
  throw new Error('окно не подписалось на прогресс')
}

function ok<T>(data: T): { status: 'ok'; data: T } {
  return { status: 'ok', data }
}

function bad<E>(error: E): { status: 'error'; error: E } {
  return { status: 'error', error }
}

function location(present: boolean) {
  return ok({
    downloads: DOWNLOADS,
    package: PACKAGE,
    inventory: INVENTORY,
    package_exists: present,
    inventory_exists: present,
  })
}

/**
 * Ответ `corpus_stats` без разбивки и без счетчиков письма.
 *
 * Такой ответ бывает на самом деле – его дает пакет, у которого нет манифеста,
 * – и проверки о двух итогах пользуются им нарочно: они про две метрики, и
 * лишние строки на экране им только мешали бы. Числа при этом настоящие: они
 * взяты у `STATS_SAMPLE`, а не написаны здесь заново.
 */
const bare: CorpusStats = {
  ...STATS_SAMPLE.manifest,
  spread: { largest: null, singletons: 0, without_cth: 0 },
  fonts: null,
}

/** Прогресс: все поля пусты, кроме названных. */
function tock(over: Partial<BuildProgress> = {}): BuildProgress {
  return {
    job: 7,
    stage: 'writing',
    done: null,
    total: null,
    manuscripts: null,
    groups: null,
    note: null,
    ...over,
  }
}

function report(over: Partial<BuildReport> = {}): BuildReport {
  return {
    job: 7,
    package: PACKAGE,
    inventory: INVENTORY,
    archive: null,
    documents: 23936,
    groups: 663,
    disambiguated: 0,
    stylesheet_dropped: 0,
    ...over,
  }
}

function failure(over: Partial<BuildFailure> = {}): BuildFailure {
  return {
    code: 'network',
    phase: 'download',
    message: 'Zenodo не ответил',
    retryable: true,
    cancelled: false,
    ...over,
  }
}

/** Обещание, которое исполняет тест: сборка длится столько, сколько нужно. */
function deferred<T>(): { promise: Promise<T>; settle: (value: T) => void } {
  let settle!: (value: T) => void
  const promise = new Promise<T>((resolve) => {
    settle = resolve
  })
  return { promise, settle }
}

/** Окно, открытое над собранным пакетом. */
async function overPackage(stats: CorpusStats = bare): Promise<HTMLElement> {
  corpusLocation.mockResolvedValue(location(true))
  corpusStats.mockResolvedValue(ok(stats))
  const { container } = render(App)
  await screen.findByRole('button', { name: 'Открыть опись' })
  return container
}

/** Окно, открытое там, где пакета нет. */
async function overNothing(): Promise<HTMLElement> {
  corpusLocation.mockResolvedValue(location(false))
  const { container } = render(App)
  await screen.findByRole('button', { name: 'Собрать' })
  return container
}

/** Нажать главную кнопку – ту, на которой держится сценарий E2E. */
async function primary(): Promise<HTMLElement> {
  return await screen.findByTestId('primary')
}

beforeEach(() => {
  vi.clearAllMocks()
  listen.mockImplementation((callback: (event: { payload: BuildProgress }) => void) => {
    emit = (payload: BuildProgress) => {
      callback({ payload })
    }
    return Promise.resolve(unlisten)
  })
  cancelBuild.mockResolvedValue(undefined)
  openPath.mockResolvedValue(undefined)
})

afterEach(cleanup)

describe('пакет есть', () => {
  /**
   * **Оба числа на экране, и оба – из ответа команды.**
   *
   * Числа взяты настоящие, из манифеста текущего пакета, и разряды проверяются
   * вместе с ними: разделитель – неразрывный пробел, поставленный самим окном,
   * а не разрядка, которую подставит чужая локаль.
   */
  it('показывает рукописи и группы из corpus_stats', async () => {
    const container = await overPackage()

    expect(screen.getByText(/Manuscripts/)).toBeInTheDocument()
    expect(screen.getByText(/Groups \(CTH\)/)).toBeInTheDocument()

    // Прямо по узлам, а не через `getByText`: тот приводит пробелы к обычным
    // перед сравнением, и неразрывный разделитель – единственное, что здесь
    // легко потерять молча, – проверять было бы нечем.
    const counts = Array.from(container.querySelectorAll('.count')).map((node) => node.textContent)
    expect(counts).toEqual(['23 936', '663'])
  })

  /**
   * **Путь окно не сочиняет.**
   *
   * `corpus_stats` получает ровно то, что вернул `corpus_location`. Вторая
   * догадка о том, где лежит пакет, разошлась бы с первой при первой же правке
   * ядра – проверка держит их одной.
   */
  it('передает в corpus_stats путь, названный corpus_location', async () => {
    await overPackage()
    expect(corpusStats).toHaveBeenCalledWith(PACKAGE)
  })

  /**
   * **Разбивка и счетчики письма встают отдельными строками.**
   *
   * Числа – настоящие, из манифеста текущего пакета: в самой большой группе
   * CTH 832 четыре с половиной тысячи фрагментов, а 116 групп состоят из
   * одного. Ради этой несоразмерности разбивка и заведена – два итога о ней
   * не говорят ничего.
   */
  it('показывает разбивку по CTH и счетчики письма', async () => {
    const container = await overPackage(STATS_SAMPLE.manifest)

    await screen.findByText(/Largest group/)
    const rows = Array.from(container.querySelectorAll('.spread')).map((node) =>
      node.textContent?.replace(/\s+/g, ' ').trim(),
    )

    expect(rows).toEqual([
      'Largest group – CTH 832 (4 480) Groups of one – 116 Without CTH – 0',
      'Not in NFC – 78 Private use – 1 269 (7 points) Anomalies – 0',
    ])
  })

  /**
   * **О том, чего не считали, экран молчит.**
   *
   * Обход каталога не читает документы, поэтому счетчиков письма у него нет, и
   * `corpus_stats` присылает `null`. Нули на их месте были бы утверждением,
   * что аномалий не нашлось, – тогда как их не искали.
   */
  it('без манифеста не показывает счетчиков письма, оставляя разбивку', async () => {
    const container = await overPackage({ ...STATS_SAMPLE.manifest, source: 'walk', fonts: null })

    await screen.findByText(/Largest group/)
    expect(container.querySelectorAll('.spread')).toHaveLength(1)
    expect(screen.queryByText(/Not in NFC/)).toBeNull()
    expect(screen.queryByText(/Anomalies/)).toBeNull()
  })

  /**
   * **Отказ команды не уносит с собой окно.**
   *
   * Текст отказа встает под заголовком, заголовок и кнопки остаются: окно
   * сообщает, что смогло, а не заменяется сообщением об ошибке целиком. Отказ
   * приходит тегированным исходом, а не брошенным исключением, – так его и
   * присылает `bindings.ts`.
   */
  it('на отказе команды показывает текст ошибки, сохраняя заголовок и кнопки', async () => {
    corpusLocation.mockResolvedValue(location(true))
    corpusStats.mockResolvedValue(bad('пакет по этому пути не найден'))

    render(App)

    expect(await screen.findByText('пакет по этому пути не найден')).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 1 })).toBeInTheDocument()
    expect(await primary()).toHaveTextContent('Собрать')
    expect(screen.queryByText(/Manuscripts/)).toBeNull()
  })

  /** **Опись открывает система, и открывает ту, что назвал `corpus_location`.** */
  it('отдает опись плагину opener', async () => {
    await overPackage()

    await fireEvent.click(screen.getByRole('button', { name: 'Открыть опись' }))

    expect(openPath).toHaveBeenCalledWith(INVENTORY)
  })
})

describe('ничего не собрано', () => {
  /**
   * **Пакета нет – окно и не спрашивает о нем чисел.**
   *
   * `corpus_stats` по несуществующему пути отказал бы, и окно показало бы
   * отказ там, где на самом деле все в порядке: собирать еще не начинали.
   * Заголовок говорит именно это, а главная кнопка предлагает начать.
   */
  it('зовет собрать и не спрашивает чисел', async () => {
    await overNothing()

    expect(corpusStats).not.toHaveBeenCalled()
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(/здесь еще нет/)
    expect(await primary()).toHaveTextContent('Собрать')
    expect(screen.getByRole('button', { name: 'Взять архив с диска…' })).toBeInTheDocument()
  })

  /**
   * **До главной кнопки доходит первая же табуляция, и с клавиатуры она
   * работает.**
   *
   * Главное действие стоит в ряду первым – это и проверяется: `click()` в
   * остальных проверках зовет обработчик, минуя фокус и клавиатуру, поэтому об
   * их работе он не говорит ничего. §4 контракта числит доступ с клавиатуры
   * среди того, что фронтенд обязан проверять.
   */
  it('дает нажать главную кнопку с клавиатуры', async () => {
    const user = userEvent.setup()
    buildCorpus.mockReturnValue(deferred<unknown>().promise)
    await overNothing()

    await user.tab()
    expect(await primary()).toHaveFocus()

    await user.keyboard('{Enter}')

    expect(buildCorpus).toHaveBeenCalledWith(null)
  })

  /**
   * **Архив с диска – выбор человека, и он же уходит в команду.**
   *
   * Фильтр по `zip` и один файл: `build_corpus` проверяет путь у себя, но
   * предлагать выбрать папку или три архива сразу окно не должно.
   */
  it('отдает выбранный архив в build_corpus', async () => {
    open.mockResolvedValue(ARCHIVE)
    buildCorpus.mockReturnValue(deferred<unknown>().promise)
    await overNothing()

    await fireEvent.click(screen.getByRole('button', { name: 'Взять архив с диска…' }))

    expect(open).toHaveBeenCalledWith({
      multiple: false,
      directory: false,
      filters: [{ name: 'Архив TLHdig', extensions: ['zip'] }],
    })
    await vi.waitFor(() => expect(buildCorpus).toHaveBeenCalledWith(ARCHIVE))
  })

  /** **Закрытый без выбора диалог – не событие.** */
  it('на отмененном выборе архива ничего не делает', async () => {
    open.mockResolvedValue(null)
    await overNothing()

    await fireEvent.click(screen.getByRole('button', { name: 'Взять архив с диска…' }))

    await vi.waitFor(() => expect(open).toHaveBeenCalled())
    expect(buildCorpus).not.toHaveBeenCalled()
    expect(await primary()).toHaveTextContent('Собрать')
  })
})

describe('идет сборка', () => {
  /** Запустить сборку и остановить окно на ней. */
  async function building(): Promise<{
    container: HTMLElement
    finish: (value: unknown) => void
  }> {
    const run = deferred<unknown>()
    buildCorpus.mockReturnValue(run.promise)
    const container = await overNothing()
    await fireEvent.click(await primary())
    await screen.findByRole('button', { name: 'Отменить' })
    return { container, finish: run.settle }
  }

  /**
   * **Пока сборка идет, окно занято ею.**
   *
   * Стадия названа по-русски, главная кнопка сменилась на отмену, и числа
   * прошлого экрана не остались: они были о другом пакете.
   */
  it('показывает стадию и предлагает отменить', async () => {
    await building()

    emit(tock({ stage: 'parsing' }))
    await tick()

    expect(screen.getByText('Разбираю архив')).toBeInTheDocument()
    expect(await primary()).toHaveTextContent('Отменить')
  })

  /**
   * **Полоса – это доля, а доля бывает не всегда.**
   *
   * Знаменатель объявляет стадия, тик заполняет числитель. Пока обеих половин
   * нет, полоса движется без деления, и `aria-valuenow` у нее отсутствует: по
   * ARIA это и означает «идет, но неизвестно сколько». Загрузка, чью длину
   * сервер не назвал, – ровно этот случай, и он настоящий.
   */
  it('двигает полосу по событию прогресса', async () => {
    const { container } = await building()

    emit(tock({ stage: 'downloading', done: 512 }))
    await tick()
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow')
    expect(container.querySelector('.bar-waiting')).not.toBeNull()

    emit(tock({ stage: 'writing', done: 5984, total: 23936 }))
    await tick()
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '25')
    expect(container.querySelector<HTMLElement>('.bar-fill')?.style.width).toBe('25%')

    emit(tock({ stage: 'writing', done: 23936, total: 23936 }))
    await tick()
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100')
    expect(container.querySelector<HTMLElement>('.bar-fill')?.style.width).toBe('100%')
  })

  /**
   * **Числа и предложение прогона держатся на экране, а не мигают.**
   *
   * «Нашлось 23 936 документов» сказано один раз, стадией `indexed`, и
   * следующее же событие стерло бы это, хотя оно не перестало быть правдой.
   */
  it('оставляет на экране числа и предложение, названные прошлой стадией', async () => {
    await building()

    emit(tock({ stage: 'zenodo-notice', note: 'запись перенесена' }))
    emit(tock({ stage: 'headers-read', manuscripts: 23936, groups: 663 }))
    emit(tock({ stage: 'writing', done: 1, total: 23936 }))
    await tick()

    expect(screen.getByText('запись перенесена')).toBeInTheDocument()
    expect(screen.getByText(/Manuscripts/)).toBeInTheDocument()
    expect(screen.getByText('23 936')).toBeInTheDocument()
    expect(screen.getByText('663')).toBeInTheDocument()
    expect(screen.getByText('Записываю документы')).toBeInTheDocument()
  })

  /**
   * **Чужой прогон на этот экран не попадает.**
   *
   * Свой номер окно узнает от первого дошедшего события – `JobId` выдается в
   * рабочем потоке, и раньше его неоткуда взять, – и все, что придет с другим
   * номером, не о том, что окно показывает.
   */
  it('не пускает на экран событие чужого прогона', async () => {
    await building()

    emit(tock({ job: 7, stage: 'parsing' }))
    await tick()
    emit(tock({ job: 8, stage: 'downloading', done: 1, total: 2 }))
    await tick()

    expect(screen.getByText('Разбираю архив')).toBeInTheDocument()
    expect(screen.queryByText('Скачиваю архив')).toBeNull()
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow')
  })

  /**
   * **Отмена подтверждается дважды.**
   *
   * Нажатие только говорит, что просьбу услышали: кнопка гаснет и меняет
   * подпись на «Останавливаю…», рядом встает строка о том, что это может
   * занять несколько секунд. «Остановлено» окно скажет отдельно – по отказу с
   * `cancelled`, пришедшему из ядра (§3 контракта).
   */
  it('на отмену говорит «Останавливаю…» и ждет подтверждения', async () => {
    const { finish } = await building()

    await fireEvent.click(await primary())

    expect(cancelBuild).toHaveBeenCalledTimes(1)
    const button = await primary()
    expect(button).toHaveTextContent('Останавливаю…')
    expect(button).toBeDisabled()
    expect(screen.getByText(/может занять несколько секунд/)).toBeInTheDocument()
    // Ядро еще не ответило – значит, «Остановлено» еще не правда.
    expect(screen.queryByText('Остановлено')).toBeNull()

    finish(bad(failure({ code: 'cancelled', cancelled: true, message: 'сборка остановлена' })))

    // По имени, а не по уровню: заголовок на экране уже есть, и `findBy` вернул
    // бы прежний, не дожидаясь ответа ядра. Ожидается смена текста, а не
    // появление элемента.
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Остановлено' }),
    ).toBeInTheDocument()
  })
})

describe('кончилось', () => {
  /** Довести сборку до названного исхода, начиная от собранного пакета. */
  async function ran(outcome: unknown, stats: CorpusStats = bare): Promise<HTMLElement> {
    buildCorpus.mockResolvedValue(outcome)
    const container = await overPackage(stats)
    await fireEvent.click(screen.getByRole('button', { name: 'Пересобрать' }))
    return container
  }

  /**
   * **На экране отчет этого прогона, а не пересчет пакета.**
   *
   * Числа приходят от самой сборки, и второй раз их никто не считает: пересчет
   * после сборки уже однажды разошелся с манифестом в этом проекте. Проверка
   * держит это тем, что `corpus_stats` знает одни числа, а отчет – другие, и
   * на экране обязаны оказаться отчетные.
   */
  it('показывает отчет прогона, а не ответ corpus_stats', async () => {
    const container = await ran(ok(report({ documents: 24001, groups: 664 })))

    await screen.findByText(/Documents/)
    const counts = Array.from(container.querySelectorAll('.count')).map((node) => node.textContent)
    expect(counts).toEqual(['24 001', '664'])
    expect(screen.queryByText(/Manuscripts/)).toBeNull()
    // Один раз, при открытии окна: после сборки числа берутся у отчета.
    expect(corpusStats).toHaveBeenCalledTimes(1)
  })

  /**
   * **Отчет говорит и о том, где пакет и из чего он собран.**
   *
   * Два младших счетчика показываются только ненулевыми: ноль здесь означает
   * «ничего такого не случилось» и занимал бы строку, ничего не сообщая.
   */
  it('называет пакет, архив и только ненулевые младшие счетчики', async () => {
    await ran(ok(report({ archive: ARCHIVE, disambiguated: 4, stylesheet_dropped: 0 })))

    expect(await screen.findByText(`Пакет – ${PACKAGE}`)).toBeInTheDocument()
    expect(screen.getByText(`Собрано из архива – ${ARCHIVE}`)).toBeInTheDocument()
    expect(screen.getByText(/Disambiguated/)).toBeInTheDocument()
    expect(screen.queryByText(/Stylesheet dropped/)).toBeNull()
  })

  /** **Опись открывается та, что назвал отчет, а не та, что нашлась при старте.** */
  it('открывает опись, названную отчетом', async () => {
    const built = `${DOWNLOADS}/TLHdig_Beta_0.4/TLHdig_Beta_0.4.html`
    await ran(ok(report({ inventory: built })))

    await screen.findByText(/Documents/)
    await fireEvent.click(screen.getByRole('button', { name: 'Открыть опись' }))

    expect(openPath).toHaveBeenCalledWith(built)
  })

  /**
   * **Опоздавшее событие законченный экран не переписывает.**
   *
   * Прогресс и отчет идут разными путями, и порядок между ними ничем не
   * гарантирован.
   */
  it('не дает опоздавшему событию стереть отчет', async () => {
    await ran(ok(report()))

    await screen.findByText(/Documents/)
    emit(tock({ stage: 'writing', done: 1, total: 23936 }))
    await tick()

    expect(screen.getByText(/Documents/)).toBeInTheDocument()
    expect(screen.queryByRole('progressbar')).toBeNull()
    expect(screen.queryByText('Записываю документы')).toBeNull()
  })

  /**
   * **Отказ, который стоит повторить, предлагает повтор – и повторяет то же.**
   *
   * Тот же архив, если его выбирали: кнопка, которая после отказа делает не то
   * же самое, ответила бы не на тот вопрос.
   */
  it('на retryable-отказе предлагает «Повторить» и повторяет тот же прогон', async () => {
    open.mockResolvedValue(ARCHIVE)
    buildCorpus.mockResolvedValue(bad(failure()))
    await overNothing()

    await fireEvent.click(screen.getByRole('button', { name: 'Взять архив с диска…' }))

    expect(await screen.findByText('Zenodo не ответил')).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Собрать не удалось')

    buildCorpus.mockClear()
    await fireEvent.click(await primary())

    expect(buildCorpus).toHaveBeenCalledWith(ARCHIVE)
  })

  /**
   * **Отказ, который повторять нечем, предлагает другой архив.**
   *
   * `retryable: false` значит, что тот же прогон кончится тем же. Единственное,
   * что здесь можно изменить, – какой архив собирать, и это и становится
   * главным действием.
   */
  it('на неповторимом отказе главным действием ставит выбор архива', async () => {
    await ran(bad(failure({ code: 'archive_missing', retryable: false, message: 'архива нет' })))

    expect(await screen.findByText('архива нет')).toBeInTheDocument()
    expect(await primary()).toHaveTextContent('Взять архив с диска…')
    expect(screen.queryByRole('button', { name: 'Повторить' })).toBeNull()
  })

  /**
   * **Остановленный прогон – не поломка.**
   *
   * `cancelled` – единственный исход, который не является неисправностью, и
   * окно говорит о нем словом «Остановлено», а не «не удалось».
   */
  it('на отмененном прогоне говорит «Остановлено», а не об ошибке', async () => {
    await ran(bad(failure({ code: 'cancelled', cancelled: true, message: 'сборка остановлена' })))

    expect(await screen.findByText('сборка остановлена')).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Остановлено')
    expect(screen.queryByText(/не удалось/)).toBeNull()
    expect(await primary()).toHaveTextContent('Собрать')
  })
})

/**
 * **Подписка не переживает окно.**
 *
 * `listen` возвращает отписку, и §4 контракта числит утекшие слушатели Tauri
 * среди того, что фронтенд обязан проверять.
 */
it('снимает подписку на прогресс при размонтировании', async () => {
  const { unmount } = render(App)
  corpusLocation.mockResolvedValue(location(false))

  await vi.waitFor(() => expect(listen).toHaveBeenCalledTimes(1))
  unmount()

  await vi.waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1))
})
