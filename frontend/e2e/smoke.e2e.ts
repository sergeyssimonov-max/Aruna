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
   * зарегистрирована в настоящем приложении, аргумент из двух слов доезжает в
   * том написании, в котором его ждет Tauri (`localArchive`, а не
   * `local_archive`, — та самая ловушка, ради которой заведена specta), а отказ
   * пересекает границу структурой, а не строкой.
   *
   * Отказом, а не успехом, и это выбрано намеренно: настоящая сборка переписала
   * бы 384 МиБ в папке загрузок того, кто гоняет проверку, и заняла бы десяток
   * секунд. Что сборка работает, держат тесты ядра и прогон на настоящем
   * корпусе; здесь проверяется провод.
   */
  it('carries a typed refusal back from the core', async () => {
    type Refusal = { code: string; retryable: boolean }
    const refusal = await browser.tauri.execute(({ core }) =>
      core
        .invoke('build_corpus', { localArchive: '/такого-архива-нет.zip' })
        .then(() => ({ code: 'resolved', retryable: false }))
        .catch((error: unknown) => error as { code: string; retryable: boolean }),
    )
    expect((refusal as Refusal).code).toBe('archive_missing')
    expect((refusal as Refusal).retryable).toBe(false)
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
