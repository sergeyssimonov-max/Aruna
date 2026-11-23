# Автоматическая сборка macOS DMG

Только **macOS Universal Binary** (Apple Silicon + Intel). Linux-сборки нет.

## Артефакты

| Файл | Описание |
|------|----------|
| `target/universal-apple-darwin/release/bundle/macos/Aruna.app` | Universal Binary, иконки из `src-tauri/icons/` |
| `target/universal-apple-darwin/release/bundle/dmg/Aruna_<version>_universal.dmg` | UDIF DMG (drag to Applications); конвейер переименовывает его в `releases/Aruna-macos-universal.dmg` и кладет рядом `SHA256SUMS` |

## Требования

- macOS 13+: локально **или** в конвейере, где задание выпуска с 14.11.2026 идет на Mac владельца, подключенном как self-hosted runner (образ `macos-14` снят с хостинга GitHub)
- Xcode 15.2 (выбрать через `DEVELOPER_DIR` или `xcode-select`) – с другим обертка сборки откажется
- Node той версии, что в `.node-version`, и pnpm
- Rust targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`

## GitHub Actions

[`.github/workflows/release-dmg.yml`](../../.github/workflows/release-dmg.yml)

| Событие | Действие |
|---------|----------|
| push / PR в `main` | Тесты и clippy, полный разбор корпуса, проверки фронтенда. DMG **не** собирается |
| `workflow_dispatch` | То же плюс Universal `.app` и DMG артефактом сборки |
| tag `v*` | То же плюс DMG и **GitHub Release** |

```bash
# ручной запуск: соберет артефакт, но релиза не выпустит
gh workflow run release-dmg.yml

# После приемки укажите номер и проверенную сумму DMG.
# CI проверит версию и сумму своей независимой сборки до публикации.
git tag -a "$tag" -m "Aruna ${tag#v}" -m "DMG-SHA256: $digest"
git push origin "refs/tags/$tag"
```

## Локально

Из корня репозитория, той же командой, что и в конвейере:

```bash
pnpm build   # scripts/build-release.sh → tauri build --target universal-apple-darwin
open target/universal-apple-darwin/release/bundle/dmg/Aruna_*_universal.dmg
```

Tauri собирает подписанное приложение. Для DMG скрипт упаковки использует время коммита и сохраненное оформление окна Finder. Порядок проверки двух чистых сборок описан в [руководстве по приемке](../../docs/TESTING.md).

## Подпись (опционально)

Apple Developer ID + `codesign` / `notarytool` — для тихой установки без Gatekeeper warning.
