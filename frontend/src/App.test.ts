import { cleanup, render, screen } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App.svelte'
import { STATS_SAMPLE } from './stats'

/**
 * Мост в Tauri, поднятый до импорта компонента.
 *
 * `vi.hoisted` здесь не украшение: фабрику `vi.mock` поднимают выше импортов,
 * и обычная переменная в этот момент еще не создана. Обе заглушки объявлены
 * вместе, потому что `App.svelte` импортирует оба модуля на верхнем уровне –
 * без второй тест падал бы на `@tauri-apps/api/window`, а не на том, что
 * проверяет.
 */
const { invoke, close } = vi.hoisted(() => ({ invoke: vi.fn(), close: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ close }) }))

const PACKAGE = '/Users/reader/Downloads/TLHdig_Beta_0.3'

const location = {
  downloads: '/Users/reader/Downloads',
  package: PACKAGE,
  inventory: `${PACKAGE}/TLHdig_Beta_0.3.html`,
  package_exists: true,
  inventory_exists: true,
}

/**
 * Ответ `corpus_stats` без разбивки и без счетчиков письма.
 *
 * Такой ответ бывает на самом деле – его дает пакет, у которого нет манифеста,
 * – и проверки о двух итогах пользуются им нарочно: они про две метрики, и
 * лишние строки на экране им только мешали бы.
 */
const bare = {
  manuscripts: 23936,
  groups: 663,
  source: 'manifest',
  spread: { largest: null, singletons: 0, without_cth: 0 },
  fonts: null,
}

beforeEach(() => {
  invoke.mockReset()
  close.mockReset()
})

afterEach(cleanup)

describe('окно готовности', () => {
  /**
   * **Оба числа на экране, и оба – из ответа команды.**
   *
   * Числа взяты настоящие, из манифеста текущего пакета, и разряды проверяются
   * вместе с ними: разделитель – неразрывный пробел, поставленный самим окном,
   * а не разрядка, которую подставит чужая локаль.
   */
  it('показывает рукописи и группы из corpus_stats', async () => {
    invoke.mockImplementation((command: string) => {
      if (command === 'corpus_location') {
        return Promise.resolve(location)
      }
      if (command === 'corpus_stats') {
        return Promise.resolve(bare)
      }
      return Promise.reject(new Error(`окно вызвало неизвестную команду ${command}`))
    })

    const { container } = render(App)

    expect(await screen.findByText(/Manuscripts/)).toBeInTheDocument()
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
    invoke.mockImplementation((command: string) => {
      if (command === 'corpus_location') {
        return Promise.resolve(location)
      }
      return Promise.resolve({ ...bare, manuscripts: 1, groups: 1, source: 'walk' })
    })

    render(App)

    await screen.findByText(/Manuscripts/)
    expect(invoke).toHaveBeenCalledWith('corpus_stats', { path: PACKAGE })
  })

  /**
   * **Отказ команды не уносит с собой окно.**
   *
   * Текст ошибки встает на место метрик, заголовок и кнопка остаются: это
   * контракт экрана, а не следствие верстки.
   */
  it('на отказе команды показывает текст ошибки, сохраняя заголовок и кнопку', async () => {
    invoke.mockImplementation((command: string) => {
      if (command === 'corpus_location') {
        return Promise.resolve(location)
      }
      // Строкой, а не `Error`, и правило линтера здесь отключено намеренно:
      // `invoke` отклоняет промис ровно тем, что вернул Rust, а `StatsError`
      // сериализуется в строку – так это и приходит в окно. Обернув строку в
      // `Error`, тест проверял бы форму, которой оболочка не присылает, и
      // `String(error)` дал бы «Error: …» вместо самого сообщения.
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
      return Promise.reject('пакет по этому пути не найден')
    })

    render(App)

    expect(await screen.findByText(/пакет по этому пути не найден/)).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 1 })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Понятно' })).toBeInTheDocument()
    expect(screen.queryByText(/Manuscripts/)).toBeNull()
  })

  /**
   * **Счетчик нажатий цел.**
   *
   * На нем держится сценарий E2E, и он растет независимо от того, чем кончится
   * закрытие окна. В jsdom `VITE_E2E` не задана, поэтому окно здесь закрывается
   * по-настоящему – через заглушку, которая это и подтверждает.
   */
  it('считает нажатия и закрывает окно', async () => {
    invoke.mockImplementation((command: string) =>
      Promise.resolve(command === 'corpus_location' ? location : bare),
    )
    close.mockResolvedValue(undefined)

    render(App)
    const button = screen.getByRole('button', { name: 'Понятно' })
    button.click()

    await vi.waitFor(() => expect(button).toHaveAttribute('data-clicks', '1'))
    expect(close).toHaveBeenCalledTimes(1)
  })

  /**
   * **До кнопки можно добраться с клавиатуры, и она оттуда работает.**
   *
   * Единственная проверка здесь, которой нужен `user-event`, и нужен по
   * существу: `button.click()` в проверке выше зовет обработчик, минуя и фокус,
   * и клавиатуру, поэтому об их работе он не говорит ничего. Экран обещает
   * обратное – в `app.css` у кнопки объявлен `:focus-visible` с обводкой, а
   * §4 контракта числит доступ с клавиатуры среди того, что фронтенд обязан
   * проверять, – и до сих пор это обещание держалось ни на чем.
   *
   * Кнопка на экране одна и других фокусируемых элементов нет, поэтому первая
   * же табуляция обязана привести именно к ней: проверяется и то, что она
   * достижима, и то, что перед ней не завелось ловушки фокуса.
   */
  it('дает нажать кнопку с клавиатуры', async () => {
    const user = userEvent.setup()
    invoke.mockImplementation((command: string) =>
      Promise.resolve(command === 'corpus_location' ? location : bare),
    )
    close.mockResolvedValue(undefined)

    render(App)
    const button = screen.getByRole('button', { name: 'Понятно' })

    await user.tab()
    expect(button).toHaveFocus()

    await user.keyboard('{Enter}')

    await vi.waitFor(() => expect(button).toHaveAttribute('data-clicks', '1'))
    expect(close).toHaveBeenCalledTimes(1)
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
    invoke.mockImplementation((command: string) =>
      Promise.resolve(command === 'corpus_location' ? location : STATS_SAMPLE.manifest),
    )

    const { container } = render(App)

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
    invoke.mockImplementation((command: string) =>
      Promise.resolve(
        command === 'corpus_location'
          ? location
          : { ...STATS_SAMPLE.manifest, source: 'walk', fonts: null },
      ),
    )

    const { container } = render(App)

    await screen.findByText(/Largest group/)
    expect(container.querySelectorAll('.spread')).toHaveLength(1)
    expect(screen.queryByText(/Not in NFC/)).toBeNull()
    expect(screen.queryByText(/Anomalies/)).toBeNull()
  })
})
