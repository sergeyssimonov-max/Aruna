# Автоматическая сборка macOS DMG

Только **macOS Universal Binary** (Apple Silicon + Intel). Linux-сборки нет.

## Артефакты

| Файл | Описание |
|------|----------|
| `Aruna.app` | Universal Binary + `.icns` из `icon.png` |
| `releases/Aruna-macos-universal.dmg` | UDIF DMG (drag to Applications) |

## Требования

- macOS 13+ (локально) **или** GitHub Actions runner `macos-14`
- Xcode CLT: `lipo`, `sips`, `iconutil`, `hdiutil`, `plutil`
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

# релиз: тег обязан совпадать с version в cli/Cargo.toml — CI это проверяет
git tag v2.5.1
git push origin v2.5.1
```

## Локально

```bash
cd cli
bash scripts/make_release.sh
open releases/Aruna-macos-universal.dmg
```

## Подпись (опционально)

Apple Developer ID + `codesign` / `notarytool` — для тихой установки без Gatekeeper warning.
