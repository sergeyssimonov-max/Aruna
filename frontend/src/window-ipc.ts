import { explain } from './failure-text'
import { open } from '@tauri-apps/plugin-dialog'
import { commands, events } from './bindings'
import type { WindowState } from './window-state.svelte'

export function windowActions(state: WindowState) {
  let revision = 0
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
      .catch(() => {
        state.trouble = 'Не удалось получать сообщения о ходе сборки.'
      })

    return () => {
      revision++
      gone = true
      unlisten?.()
      unlisten = null
    }
  }

  async function load(): Promise<void> {
    const reading = ++revision
    try {
      const located = await commands.corpusLocation()
      if (located.status === 'error') {
        state.trouble = located.error
        state.screen = { kind: 'unreadable', inventory: null }
        return
      }
      state.destination = located.data.destination ?? null
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
      const [parsed, edition] = await Promise.allSettled([
        commands.corpusXml(located.data.package),
        commands.corpusEdition(located.data.package),
      ])
      if (reading !== revision) return
      state.edition =
        edition.status === 'fulfilled' && edition.value.status === 'ok' ? edition.value.data : null
      // Опись отдельным флагом, как и в ветке `unreadable` выше: два флага
      // бывают разными – каталог пакета на месте, описи в нем уже нет, – и
      // кнопка над отсутствующим файлом могла кончиться только отказом.
      state.screen = {
        kind: 'present',
        stats: counted.data,
        inventory: located.data.inventory_exists ? located.data.inventory : null,
        markup:
          parsed.status === 'fulfilled' && parsed.value.status === 'ok' ? parsed.value.data : null,
      }
      if (state.edition !== null && located.data.inventory_exists) {
        // The saved inventory is already openable while Zenodo answers.
        void refresh(located.data.package, reading)
      }
    } catch {
      state.trouble = 'Не удалось прочитать сведения о пакете.'
      state.screen = { kind: 'unreadable', inventory: null }
    }
  }

  async function refresh(path: string, reading: number): Promise<void> {
    try {
      const update = await commands.corpusUpdate(path)
      if (revision !== reading || state.screen.kind !== 'present') return
      if (update.status === 'error') state.trouble = update.error
      else if (update.data) await build(state.destination, true)
    } catch {
      if (revision === reading) state.trouble = 'Не удалось проверить новые издания на Zenodo.'
    }
  }

  async function build(chosen: string | null, automatic = false): Promise<void> {
    revision++
    const previous = automatic && state.screen.kind === 'present' ? state.screen : null
    function failed(failure: import('./bindings').BuildFailure) {
      state.finish()
      if (previous !== null) {
        state.screen = previous
        state.trouble = failure.cancelled
          ? 'Обновление остановлено. Можно открыть прежнюю опись.'
          : explain(failure) + ' Можно открыть прежнюю опись.'
      } else {
        state.screen = { kind: 'failed', failure }
      }
    }
    state.begin(chosen, previous?.inventory ?? null)
    try {
      const built = automatic ? await commands.buildUpdate() : await commands.buildCorpus(chosen)
      if (built.status === 'ok') {
        state.finish(built.data.job)
        // Итоги сборки дает отчет. Разбивку и пояснения читаем из того пакета,
        // который только что опубликован, в том числе при выборе другой папки.
        const [stats, xml, edition] = await Promise.allSettled([
          commands.corpusStats(built.data.package),
          commands.corpusXml(built.data.package),
          commands.corpusEdition(built.data.package),
        ])
        state.edition =
          edition.status === 'fulfilled' && edition.value.status === 'ok'
            ? edition.value.data
            : null
        state.screen = {
          kind: 'built',
          report: built.data,
          spread:
            stats.status === 'fulfilled' && stats.value.status === 'ok'
              ? stats.value.data.spread
              : null,
          markup: xml.status === 'fulfilled' && xml.value.status === 'ok' ? xml.value.data : null,
        }
      } else {
        failed(built.error)
      }
    } catch (error: unknown) {
      // Опоздавшие события этого прогона возможны и после обрыва моста.
      failed({
        code: 'broken',
        phase: null,
        message: String(error),
        retryable: true,
        cancelled: false,
      })
    } finally {
      state.stopping = false
    }
  }

  /** Папка для пакета. Отмененный выбор – не событие: окно остается как было. */
  async function pickFolder(): Promise<void> {
    revision++
    try {
      const chosen = await open({ multiple: false, directory: true })
      if (chosen === null) {
        return
      }
      await build(chosen)
    } catch {
      state.trouble = 'Не удалось открыть окно выбора папки.'
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
    } catch {
      // `cancel_build` в Rust возвращает unit и отказать не может, так что
      // сюда попадает только сорванный вызов – а значит, никто ничего не
      // просил, и кнопка обязана вернуться.
      state.stopping = false
      state.trouble = 'Не удалось передать просьбу об остановке. Нажмите «Отменить» еще раз.'
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
    } catch {
      state.trouble = 'Не удалось открыть опись. Попробуйте еще раз.'
    }
  }

  return { subscribe, load, build, pickFolder, stop, reveal }
}
