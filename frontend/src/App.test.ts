import { cleanup, fireEvent, render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { tick } from 'svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App.svelte'
import { STATS_SAMPLE } from './stats'
import type { BuildFailure, BuildProgress, BuildReport, CorpusStats, XmlSummary } from './bindings'

/**
 * Мост в Tauri – один модуль, и заглушка тоже одна.
 *
 * `bindings.ts` порожден tauri-specta и с этого дня единственная поверхность
 * Tauri, которую окно трогает напрямую: подменять `@tauri-apps/api/core` и
 * `@tauri-apps/api/event` порознь значило бы описывать в тесте еще и рантайм
 * specta – два тегированных исхода, `makeEvent` и приведение полезной нагрузки,
 * – ничего из чего окно не выбирает.
 *
 * Отдельно остается один плагин – `dialog`: его окно зовет само, минуя
 * `bindings.ts`, и в этом весь смысл его подмены – проверить, что зовет именно
 * его. `opener` из этого списка ушел 07.09.2026: опись открывает команда ядра,
 * и подменять здесь больше нечего. Подмена и была той слепотой, из-за которой
 * кнопка «Открыть опись» не работала ни в одной выпущенной сборке, а 88 тестов
 * этого не видели: за `openPath` стояла область путей, которой заглушка нет.
 *
 * `vi.hoisted` здесь не украшение: фабрику `vi.mock` поднимают выше импортов, и
 * обычная переменная в этот момент еще не создана.
 */
const {
  corpusLocation,
  corpusStats,
  corpusXml,
  buildCorpus,
  cancelBuild,
  listen,
  unlisten,
  open,
  openInventory,
} = vi.hoisted(() => ({
  corpusLocation: vi.fn(),
  corpusStats: vi.fn(),
  corpusXml: vi.fn(),
  buildCorpus: vi.fn(),
  cancelBuild: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
  open: vi.fn(),
  openInventory: vi.fn(),
}))

vi.mock('./bindings', () => ({
  commands: { corpusLocation, corpusStats, corpusXml, openInventory, buildCorpus, cancelBuild },
  events: { buildProgress: { listen } },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open }))

const DOWNLOADS = '/Users/reader/Downloads'
const PACKAGE = `${DOWNLOADS}/TLHdig_Beta_0.3`
const INVENTORY = `${PACKAGE}/TLHdig_Beta_0.3.html`
const FOLDER = '/Users/reader/Документов/Корпус'

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

/** Сводка о разметке, какой ее отдает манифест собранного пакета. */
const markup: XmlSummary = {
  documents: 23936,
  well_formed: 23730,
  not_well_formed: 206,
  reasons: [
    { reason: 'unterminated-start-tag', documents: 95 },
    { reason: 'element-never-closed', documents: 33 },
    // Причина с нулем приходит с провода и обязана не попасть на экран:
    // строка «0 документов» читается как найденная беда, а не как ее
    // отсутствие.
    { reason: 'unclassified', documents: 0 },
  ],
  documents_not_well_formed: [
    { file: 'CTH 341/KBo 22.91.xml', reason: 'element-never-closed', line: 12, column: 7 },
    { file: 'CTH 448/KBo 15.15.xml', reason: 'crossing-elements', line: 3, column: 40 },
  ],
}

/** Окно, открытое над собранным пакетом. */
async function overPackage(
  stats: CorpusStats = bare,
  xml: XmlSummary | null = markup,
): Promise<HTMLElement> {
  corpusLocation.mockResolvedValue(location(true))
  corpusStats.mockResolvedValue(ok(stats))
  corpusXml.mockResolvedValue(
    xml === null ? bad('манифест пакета не содержит сведений о разметке') : ok(xml),
  )
  const { container } = render(App)
  await screen.findByRole('button', { name: 'Открыть опись' })
  return container
}

/** Окно, открытое там, где пакета нет. */
async function overNothing(): Promise<HTMLElement> {
  corpusLocation.mockResolvedValue(location(false))
  const { container } = render(App)
  await screen.findByRole('button', { name: 'Собрать по умолчанию' })
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
  openInventory.mockResolvedValue(ok(null))
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

    expect(screen.getByText(/Рукописей/)).toBeInTheDocument()
    expect(screen.getByText(/Групп CTH/)).toBeInTheDocument()

    // Прямо по узлам, а не через `getByText`: тот приводит пробелы к обычным
    // перед сравнением, и неразрывный разделитель – единственное, что здесь
    // легко потерять молча, – проверять было бы нечем.
    // Блок разметки исключен: с 06.09 у него свои счетчики, а этот тест про
    // два числа заголовка. Селектор без оговорки собрал бы все подряд и
    // ломался бы от любой новой строки на экране.
    const counts = Array.from(container.querySelectorAll('.count'))
      .filter((node) => !node.closest('.markup'))
      .map((node) => node.textContent)
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
   * **Разбивка встает отдельной строкой.**
   *
   * Числа – настоящие, из манифеста текущего пакета: в самой большой группе
   * CTH 832 четыре с половиной тысячи фрагментов, а 116 групп состоят из
   * одного. Ради этой несоразмерности разбивка и заведена – два итога о ней
   * не говорят ничего.
   */
  it('показывает разбивку по CTH', async () => {
    const container = await overPackage(STATS_SAMPLE.manifest)

    await screen.findByText(/Самая большая группа/)
    const rows = Array.from(container.querySelectorAll('.spread'))
      .filter((node) => !node.closest('.markup'))
      .map((node) => node.textContent?.replace(/\s+/g, ' ').trim())

    expect(rows).toEqual(['Самая большая группа – CTH 832 (4 480) Групп из одной рукописи – 116'])
  })

  /**
   * **На экране пакета ровно пять чисел, и это решение, а не то, что вышло.**
   *
   * 06.09.2026 их было тринадцать: к пяти нынешним прибавлялись рукописи без
   * CTH, четыре счетчика письма, разбивка по причинам и список из двухсот
   * имен под раскрытием. Каждое из них было верным и ни одно не было нужным
   * в первую секунду: экран отвечает на вопрос «что получилось», а не заменяет
   * собой манифест, где все это лежит по-прежнему.
   *
   * Тест перечисляет строки целиком, а не ищет пропавшее по одному: список
   * того, чего нет, растет с каждым снятым счетчиком и ничего не говорит о
   * том, что осталось. Про счетчики письма он заодно и есть последняя
   * проверка – они с экрана сняты, а с провода нет.
   */
  it('показывает ровно пять чисел и ничего больше', async () => {
    const container = await overPackage(STATS_SAMPLE.manifest)
    await screen.findByText(/Самая большая группа/)

    const plain = (node: Element) => node.textContent?.replace(/\s+/g, ' ').trim()

    expect(Array.from(container.querySelectorAll('.count')).map(plain)).toEqual([
      '23 936',
      '663',
      'CTH 832',
      '116',
      '206',
    ])
    expect(Array.from(container.querySelectorAll('.metrics, .spread')).map(plain)).toEqual([
      'Рукописей – 23 936 Групп CTH – 663',
      'Самая большая группа – CTH 832 (4 480) Групп из одной рукописи – 116',
      'Некорректных XML – 206 из 23 936',
    ])
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
    expect(screen.queryByText(/Рукописей/)).toBeNull()
  })

  /**
   * **Сводка о разметке видна, и видно, что документы никуда не делись.**
   *
   * Числа – это половина дела. Вторая половина в том, что читатель не должен
   * уйти с мыслью, будто 206 документов из пакета выброшены: они там, и экран
   * обязан сказать это словами, а не умолчанием.
   *
   * Фраза снималась на несколько часов 06.09.2026, когда экран сводили к пяти
   * числам, и возвращена решением владельца в тот же день. Это единственная
   * строка на экране, которая не число, и держать ее тестом стоит именно
   * поэтому: снять ее легко и незаметно, а без нее `206` читается как
   * «206 выброшено».
   */
  it('показывает, сколько документов не корректный XML, и что они в пакете', async () => {
    await overPackage()

    expect(corpusXml).toHaveBeenCalledWith(PACKAGE)
    expect(await screen.findByText(/Некорректных XML/)).toBeInTheDocument()
    expect(screen.getByText('206')).toBeInTheDocument()
    // Не только числа: экран обязан сказать словами, что документы на месте.
    expect(screen.getByText(/лежат в пакете вместе с остальными/i)).toBeInTheDocument()
    // Ни одного слова, приписывающего документу то, чего с ним не делали.
    // Список русский с 06.09.2026, вместе с самим экраном: английские слова
    // после перевода не встретились бы никогда и проверку обессмыслили бы.
    // Основы, а не формы, и «некорректный» в список не входит – так эти
    // документы называет спецификация.
    const shown = (document.body.textContent ?? '').toLowerCase()
    for (const word of ['отклон', 'отверг', 'исключ', 'испорч', 'поврежд', 'сломан', 'ошибк']) {
      expect(shown).not.toContain(word)
    }
  })

  /**
   * **Пакет, собранный до 06.09.2026, открывается как обычно.**
   *
   * Его манифест сведений о разметке не несет, и это не поломка окна. Отказ
   * `corpus_xml` не имеет права увести экран в `unreadable`: числа рукописей
   * на месте, опись открывается, отсутствует только один раздел.
   */
  it('без сведений о разметке в манифесте показывает пакет как обычно', async () => {
    await overPackage(bare, null)

    expect(await primary()).toHaveTextContent('Открыть опись')
    expect(screen.queryByText(/Некорректных XML/)).toBeNull()
  })

  /** **Опись открывает система, и открывает ту, что назвал `corpus_location`.** */
  it('отдает опись плагину opener', async () => {
    await overPackage()

    await fireEvent.click(screen.getByRole('button', { name: 'Открыть опись' }))

    expect(openInventory).toHaveBeenCalledWith(INVENTORY)
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
    expect(await primary()).toHaveTextContent('Собрать по умолчанию')
    expect(screen.getByRole('button', { name: 'Собрать в папку…' })).toBeInTheDocument()
  })

  /**
   * **Взять архив с диска окно не предлагает нигде.**
   *
   * Источник один – портал, решением владельца 06.09.2026. Держит это форма
   * команды: единственный ее аргумент – папка назначения, назвать архив
   * нечем. Проверка стоит на том, что видит человек: кнопка, которая
   * вернулась бы вместе с выбором.
   */
  it('нигде не предлагает взять архив с диска', async () => {
    await overNothing()

    expect(screen.queryByRole('button', { name: /архив/i })).toBeNull()
  })

  /**
   * **Папка выбирается, и выбранная уходит в команду.**
   *
   * Каталог, а не файл, и один: `build_corpus` проверяет путь у себя, но
   * предлагать выбрать три папки сразу окно не должно. Источник при этом не
   * трогается – аргумента, которым его можно назвать, у команды нет.
   */
  it('отдает выбранную папку в build_corpus', async () => {
    open.mockResolvedValue(FOLDER)
    buildCorpus.mockReturnValue(deferred<unknown>().promise)
    await overNothing()

    await fireEvent.click(screen.getByRole('button', { name: 'Собрать в папку…' }))

    expect(open).toHaveBeenCalledWith({ multiple: false, directory: true })
    await vi.waitFor(() => expect(buildCorpus).toHaveBeenCalledWith(FOLDER))
  })

  /** **Закрытый без выбора диалог – не событие: окно остается как было.** */
  it('на отмененном выборе папки ничего не делает', async () => {
    open.mockResolvedValue(null)
    await overNothing()

    await fireEvent.click(screen.getByRole('button', { name: 'Собрать в папку…' }))

    await vi.waitFor(() => expect(open).toHaveBeenCalled())
    expect(buildCorpus).not.toHaveBeenCalled()
    expect(await primary()).toHaveTextContent('Собрать по умолчанию')
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
    expect(screen.getByText(/Рукописей/)).toBeInTheDocument()
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

    await screen.findByText(/Документов/)
    const counts = Array.from(container.querySelectorAll('.count')).map((node) => node.textContent)
    expect(counts).toEqual(['24 001', '664'])
    expect(screen.queryByText(/Рукописей/)).toBeNull()
    // Один раз, при открытии окна: после сборки числа берутся у отчета.
    expect(corpusStats).toHaveBeenCalledTimes(1)
  })

  /**
   * **Отчет говорит, где пакет, и двумя числами кончается.**
   *
   * Проверка перевернута 08.09.2026, а не удалена: до того дня она требовала,
   * чтобы оба младших счетчика стояли на экране, – теперь требует, чтобы их
   * там не было, и держит границу с той же стороны, с какой ее сдвинули.
   *
   * Числа поданы ненулевыми нарочно. Нулями тест прошел бы и на старом коде –
   * до 07.09.2026 ноль прятал строку сам, – и о снятии не сказал бы ничего.
   * По проводу они идут по-прежнему: сняты с экрана, как счетчики письма
   * 06.09.2026, а не из отчета.
   */
  it('называет пакет, и младших счетчиков на экране нет', async () => {
    const container = await ran(ok(report({ disambiguated: 4, stylesheet_dropped: 7 })))

    expect(await screen.findByText(`Пакет – ${PACKAGE}`)).toBeInTheDocument()
    expect(screen.queryByText(/Переименовано из-за совпадения/)).toBeNull()
    expect(screen.queryByText(/С лишней ссылкой на оформление/)).toBeNull()
    const counts = Array.from(container.querySelectorAll('.count')).map((n) => n.textContent)
    expect(counts).toEqual(['23 936', '663'])
  })

  /**
   * **Отказ открытия виден в окне, и он на русском языке.**
   *
   * До 07.09.2026 сюда попадало `Not allowed to open path /Users/…` – язык не
   * тот и путь внутри, вопреки §3 контракта. Проверка держит оба обещания, а не
   * текст сообщения: текст принадлежит `CommandError` и проверяется у него.
   */
  it('показывает отказ открытия по-русски и без пути', async () => {
    openInventory.mockResolvedValue(bad('описи нет на месте – соберите корпус заново'))
    await ran(ok(report()))

    await screen.findByText(/Документов/)
    await fireEvent.click(screen.getByRole('button', { name: 'Открыть опись' }))

    const said = await screen.findByText(/описи нет на месте/)
    expect(said).toBeInTheDocument()
    expect(said.textContent ?? '').not.toContain('/')
  })

  /** **Опись открывается та, что назвал отчет, а не та, что нашлась при старте.** */
  it('открывает опись, названную отчетом', async () => {
    const built = `${DOWNLOADS}/TLHdig_Beta_0.4/TLHdig_Beta_0.4.html`
    await ran(ok(report({ inventory: built })))

    await screen.findByText(/Документов/)
    await fireEvent.click(screen.getByRole('button', { name: 'Открыть опись' }))

    expect(openInventory).toHaveBeenCalledWith(built)
  })

  /**
   * **Опоздавшее событие законченный экран не переписывает.**
   *
   * Прогресс и отчет идут разными путями, и порядок между ними ничем не
   * гарантирован.
   */
  it('не дает опоздавшему событию стереть отчет', async () => {
    await ran(ok(report()))

    await screen.findByText(/Документов/)
    emit(tock({ stage: 'writing', done: 1, total: 23936 }))
    await tick()

    expect(screen.getByText(/Документов/)).toBeInTheDocument()
    expect(screen.queryByRole('progressbar')).toBeNull()
    expect(screen.queryByText('Записываю документы')).toBeNull()
  })

  /**
   * **Отказ, который стоит повторить, предлагает повтор – и повторяет то же.**
   *
   * Ту же закрепленную запись: кнопка, которая после отказа делает не то же
   * самое, ответила бы не на тот вопрос.
   */
  it('на retryable-отказе предлагает «Повторить» и повторяет тот же прогон', async () => {
    await ran(bad(failure()))

    expect(await screen.findByText('Zenodo не ответил')).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Собрать не удалось')

    buildCorpus.mockClear()
    await fireEvent.click(await primary())

    expect(buildCorpus).toHaveBeenCalledWith(null)
  })

  /**
   * **Отказ, который повторять нечем, оставляет одно действие – собрать снова.**
   *
   * `retryable: false` значит, что тот же прогон кончится тем же, и менять окну
   * нечего: источник один. Кнопка все же есть – переполненный диск и занятый
   * каталог вывода чинятся снаружи окна, а начать заново после починки нужно
   * чем-то. Экран без единого действия был бы тупиком, и проверка сторожит
   * именно это.
   */
  it('на неповторимом отказе оставляет главным действием «Собрать»', async () => {
    await ran(bad(failure({ code: 'distorted', retryable: false, message: 'пакет искажен' })))

    expect(await screen.findByText('пакет искажен')).toBeInTheDocument()
    expect(await primary()).toHaveTextContent('Собрать по умолчанию')
    expect(screen.queryByRole('button', { name: 'Повторить' })).toBeNull()
  })

  /**
   * **Остановленный прогон – не поломка.**
   *
   * `cancelled` – единственный исход, который не является неисправностью, и
   * окно говорит о нем словом «Остановлено», а не «не удалось».
   *
   * Подпись здесь – «Собрать», а не «Собрать по умолчанию»: кнопка повторяет
   * прогон с той же папкой, какую выбирали, и обещать умолчание она не
   * вправе. Правило одно на все окно – подпись следует за поведением:
   * «по умолчанию» стоит там и только там, где зовут `build(null)`.
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
  corpusLocation.mockResolvedValue(location(false))
  const { unmount } = render(App)

  await vi.waitFor(() => expect(listen).toHaveBeenCalledTimes(1))
  unmount()

  await vi.waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1))
})
