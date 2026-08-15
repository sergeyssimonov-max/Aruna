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
| push / PR в `main` | Сборка на `macos-14` → artifact DMG |
| `workflow_dispatch` | То же вручную |
| tag `v1.0.3` | DMG + **GitHub Release** |

```bash
# ручной запуск
gh workflow run release-dmg.yml

# релиз
git tag v1.0.3
git push origin v1.0.3
```

## Локально

```bash
cd cli
bash scripts/make_release.sh
open releases/Aruna-macos-universal.dmg
```

## Подпись (опционально)

Apple Developer ID + `codesign` / `notarytool` — для тихой установки без Gatekeeper warning.
