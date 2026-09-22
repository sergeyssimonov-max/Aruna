import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

/**
 * Версия, которую приложение обязано о себе сообщать.
 *
 * Читается из манифеста оболочки, а не пишется здесь числом: поле `version`
 * убрано из `tauri.conf.json`, и манифест остался единственным местом, откуда
 * Tauri эту версию берет. Сверка с записанной константой проверяла бы, что
 * никто не менял константу, а не то, что приложение говорит правду о себе.
 */
function shellVersion(): string {
  const manifest = readFileSync(
    fileURLToPath(new URL('../../src-tauri/Cargo.toml', import.meta.url)),
    'utf8',
  )
  const match = manifest.match(/^\s*version\s*=\s*"([^"]+)"/m)
  if (!match) {
    throw new Error('src-tauri/Cargo.toml не объявляет версию')
  }
  return match[1]
}

describe('Aruna window', () => {
  it('opens and shows the interface', async () => {
    const primary = await $('[data-testid="primary"]')
    await expect(primary).toExist()
  })

  it('reaches the Tauri backend', async () => {
    const kind = await browser.tauri.execute(({ core }) => typeof core.invoke)
    expect(kind).toBe('function')
    const version = await browser.tauri.execute(({ core }) => core.invoke('plugin:app|version'))
    expect(version).toBe(shellVersion())
  })

  /**
   * Сборка доходит до ядра и возвращается типизированным отказом.
   *
   * Проверяется здесь то, чего не может jsdom: команда действительно
   * зарегистрирована в настоящем приложении, аргумент доезжает в том написании,
   * в котором его ждет Tauri, и отказ пересекает границу структурой, а не
   * строкой.
   *
   * Аргумент за неделю сменил смысл дважды. До 06.09.2026 он назывался
   * `localArchive` и был источником; источник снят решением владельца, и
   * сценарий на время переезжал на `corpus_stats`. Теперь аргумент есть снова и
   * означает другое – папку, куда класть пакет, – а источник по-прежнему один.
   * Отказ на несуществующей папке стоит ровно ту же проверку и не строит
   * ничего: 384 МиБ в чужой папке загрузок и десяток секунд здесь недопустимы.
   */
  it('carries a typed refusal back from the core', async () => {
    type Refusal = { code: string; retryable: boolean }
    const refusal = await browser.tauri.execute(({ core }) =>
      core
        .invoke('build_corpus', { destination: '/такой-папки-нет' })
        .then(() => ({ code: 'resolved', retryable: false }))
        .catch((error: unknown) => error as { code: string; retryable: boolean }),
    )
    expect((refusal as Refusal).code).toBe('destination_missing')
    expect((refusal as Refusal).retryable).toBe(false)
  })

  /**
   * **Окну выдано только то, что оно зовет** (с 22.09.2026, находка аудита
   * 21.09). Плагины, которыми пользуется одна Rust-сторона, из страницы
   * недостижимы: проверка разрешений стоит в настоящем приложении, и jsdom ее
   * не видит. Вызовы выбраны безвредными – если граница не держит, они только
   * читают.
   */
  it('refuses the page the plugins only the shell uses', async () => {
    const said = await browser.tauri.execute(({ core }) =>
      Promise.all(
        (
          [
            ['plugin:store|get_store', { path: 'e2e.json' }],
            ['plugin:window-state|filename', {}],
            ['plugin:opener|reveal_item_in_dir', { paths: ['/такого-пути-нет'] }],
          ] as const
        ).map(([command, args]) =>
          core.invoke(command, args).then(
            () => `${command}: resolved`,
            (error: unknown) => `${command}: ${String(error)}`,
          ),
        ),
      ),
    )
    expect(said).toHaveLength(3)
    for (const line of said as string[]) {
      expect(line).toMatch(/not allowed/i)
    }
  })

  /** Читающие команды читают только каталог с именем пакета. */
  it('reads nothing but a directory named as the package', async () => {
    const said = await browser.tauri.execute(({ core }) =>
      core.invoke('corpus_stats', { path: '/etc' }).then(
        () => 'resolved',
        (error: unknown) => String(error),
      ),
    )
    expect(said).toBe('это не пакет корпуса')
  })

  /** Остановить нечего — и это не ошибка. */
  it('takes a stop for a build that is not running', async () => {
    const stopped = await browser.tauri.execute(({ core }) =>
      core.invoke('cancel_build').then(
        () => 'ok',
        () => 'rejected',
      ),
    )
    expect(stopped).toBe('ok')
  })
})
