# Автоматическая сборка DMG

## Два формата

| Артефакт | Где собирается | Формат |
|----------|----------------|--------|
| `Aruna.dmg` | Linux CI (`pack_dmg.py`) | ISO9660 + Joliet + Rock Ridge |
| `Aruna-macos-universal.dmg` | **macOS** runner (`hdiutil`) | Настоящий UDIF + Universal `.app` |

Настоящий macOS-DMG **нельзя** надёжно собрать на Linux: нужны `lipo`, `iconutil`, `hdiutil`.

## GitHub Actions

Workflow: [`.github/workflows/release-dmg.yml`](../../.github/workflows/release-dmg.yml)

| Событие | Что происходит |
|---------|----------------|
| `push` / PR в `main` | Linux: `cargo build --release` → `make_release.sh` → artifact |
| `workflow_dispatch` | Linux + **macOS** Universal `.app` + UDIF DMG |
| tag `v1.0.0` | Оба пакета + **GitHub Release** с файлами |

### Включить

1. Запушить workflow (уже в репозитории).
2. GitHub → **Actions** → разрешить workflows при запросе.
3. Проверка без релиза: Actions → *Release DMG* → **Run workflow**.
4. Релиз:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```

### Локально

```bash
# Linux / CI-совместимый DMG
cd cli && bash scripts/make_release.sh

# Только macOS 13+:
cd cli && ./build_app.sh
```

### Подпись (опционально)

Для Gatekeeper: Apple Developer ID + `codesign` / `notarytool` и secrets `APPLE_*` в job `macos-dmg`.
