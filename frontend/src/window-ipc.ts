import { open } from '@tauri-apps/plugin-dialog'
import { commands, events } from './bindings'
import type { WindowState } from './window-state.svelte'

export function windowActions(state: WindowState) {
  function subscribe(): () => void {
    let unlisten: (() => void) | null = null
    // Подписка может вернуться уже после закрытия окна.
    let gone = false

    void events.buildProgress
      .listen((event) => {
        state.advance(event.payload)
      })
      .then((stop) => {
        if (gone) {
          stop()
        } else {
          unlisten = stop
        }
      })
      .catch((error: unknown) => {
        state.trouble = String(error)
      })

    return () => {
      gone = true
      unlisten?.()
      unlisten = null
    }
  }

  async function load(): Promise<void> {
    try {
      const located = await commands.corpusLocation()
      if (located.status === 'error') {
        state.trouble = located.error
        state.screen = { kind: 'unreadable', inventory: null }
        return
      }
      if (!located.data.package_exists) {
        state.screen = { kind: 'absent' }
        return
      }
      const counted = await commands.corpusStats(located.data.package)
      if (counted.status === 'error') {
        state.trouble = counted.error
        // Опись открыть все еще можно, если она на месте: числа не сошлись, а
        // документ – отдельная вещь и от них не зависит.
        state.screen = {
          kind: 'unreadable',
          inventory: located.data.inventory_exists ? located.data.inventory : null,
        }
        return
      }
      // В старом манифесте может отсутствовать xml. Числа и опись доступны и без него.
      const parsed = await commands.corpusXml(located.data.package)
      // Опись отдельным флагом, как и в ветке `unreadable` выше: два флага
      // бывают разными – каталог пакета на месте, описи в нем уже нет, – и
      // кнопка над отсутствующим файлом могла кончиться только отказом.
      state.screen = {
        kind: 'present',
        stats: counted.data,
        inventory: located.data.inventory_exists ? located.data.inventory : null,
        markup: parsed.status === 'ok' ? parsed.data : null,
      }
    } catch (error: unknown) {
      state.trouble = String(error)
      state.screen = { kind: 'unreadable', inventory: null }
    }
  }

  async function build(chosen: string | null): Promise<void> {
    state.begin(chosen)
    try {
      const built = await commands.buildCorpus(chosen)
      if (built.status === 'ok') {
        state.finish(built.data.job)
        // Только при отказах PDF окно сверяет их число с отказами модели,
        // чтобы не показывать читателю один и тот же итог дважды.
        let objected: number | null = null
        if (built.data.pdfs.refused > 0) {
          try {
            const parsed = await commands.corpusXml(built.data.package)
            objected = parsed.status === 'ok' ? parsed.data.objected_to : null
          } catch {
            objected = null
          }
        }
        state.screen = { kind: 'built', report: built.data, objected }
      } else {
        state.finish()
        state.screen = { kind: 'failed', failure: built.error }
      }
    } catch (error: unknown) {
      // Опоздавшие события этого прогона возможны и после обрыва моста.
      state.finish()
      state.screen = {
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
      state.stopping = false
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
      state.trouble = String(error)
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
    state.stopping = true
    try {
      await commands.cancelBuild()
    } catch (error: unknown) {
      // `cancel_build` в Rust возвращает unit и отказать не может, так что
      // сюда попадает только сорванный вызов – а значит, никто ничего не
      // просил, и кнопка обязана вернуться.
      state.stopping = false
      state.trouble = String(error)
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
        state.trouble = opened.error
      }
    } catch (error: unknown) {
      state.trouble = String(error)
    }
  }

  return { subscribe, load, build, pickFolder, stop, reveal }
}
