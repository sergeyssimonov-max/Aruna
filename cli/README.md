# Aruna

**Aruna** — компактная CLI-утилита на Rust, которая автоматически:

1. скачивает корпус [TLHdig Beta 0.3](https://zenodo.org/records/20328284) с Zenodo (~74 МБ ZIP);
2. распаковывает и эвристически парсит все XML-транслитерации хеттских клинописных манускриптов;
3. раскладывает документы по папкам CTH и собирает над ними минималистичную
   HTML-опись в скандинавском стиле, где каждая строка ссылается на свой XML;
4. сохраняет результат в  
   `~/Downloads/TLHdig_Beta_0.3/`, опись — файл `TLHdig_Beta_0.3.html` внутри
   этой папки.

Запуск **без аргументов**. По завершении:

```text
Готово.
  Корпус: /Users/…/Downloads/TLHdig_Beta_0.3
  Опись:  /Users/…/Downloads/TLHdig_Beta_0.3/TLHdig_Beta_0.3.html
  рукописей: 23936, групп: 663
```

Отдельной описи рядом с папкой программа не пишет: до 2.3.0 писала, и из двух
файлов с одним именем читатель открывал тот, в котором не было ни одной ссылки.

Источник данных:  
`https://zenodo.org/records/20328284/files/TLHbasisONLINE25_1_ZENODO_Beta_03.zip`

---

## Требования

| | |
|---|---|
| Rust | 1.86+ (`rustup`) — минимум задаётся зависимостями `ureq`, не нашим кодом |
| ОС для CLI | macOS 13+ и Linux — собираются и тестируются в CI; на Windows не проверялось |
| ОС для `.app` | **macOS 13 Ventura+** (Apple Silicon и Intel) |
| Сеть | доступ к `zenodo.org` при первом запуске; дальше архив берётся из кэша |

---

## Быстрый старт (CLI)

```bash
git clone https://github.com/sergeyssimonov-max/Aruna.git
cd cli
cargo build --release
./target/release/aruna
```

Офлайн / свой ZIP:

```bash
ARUNA_ZIP=/path/to/TLHbasisONLINE25_1_ZENODO_Beta_03.zip ./target/release/aruna
```

### Что берётся из API Zenodo

Перед загрузкой (и только перед ней — прогон с готовым кэшем сети не касается)
делается один запрос к `/api/records/20328284/versions/latest`. Он отвечает
сразу на два вопроса: не вышла ли новая редакция корпуса и какую контрольную
сумму Zenodo публикует для нашей.

Оба ответа — совещательные. Прибитая в коде сумма остаётся главной: она
фиксирует ту редакцию, на которой проверен парсер, и если брать сумму из API,
проверка «тот ли это архив» превратится в «не побилось ли при передаче», а
перевыпущенный корпус пройдёт молча. Недоступный API прогон не останавливает.

Отдельных XML-документов API не отдаёт — в записи лежит один ZIP. Читать его
частями через Range-запросы технически можно, но это 23 937 запросов при лимите
133 в минуту: около трёх часов ради экономии 29 МБ против 65 секунд сплошной
загрузки.

### Кэш архива

Первый запуск скачивает 71 МБ и **оставляет их** в кэше ОС
(`~/Library/Caches/aruna` на macOS, `~/.cache/aruna` на Linux). Следующие
запуски сети не требуют вовсе: ~1 с вместо минуты.

Файл назван по контрольной сумме, которую обязан иметь, и она пересчитывается
при каждом попадании — 0.27 с на 71 МБ. Поэтому перевыпуск архива на Zenodo не
может быть отдан из кэша: другая сумма — другое имя, а прежний файл удаляется,
когда рядом ложится новый.

```bash
ARUNA_CACHE_DIR=/tmp/aruna-cache ./target/release/aruna   # кэш в другом месте
rm -rf ~/Library/Caches/aruna                             # выбросить кэш
```

Чистильщики вроде CleanMyMac этот каталог опустошают — так и задумано: файл
восстановим, и место должно освобождаться штатными средствами. Ничего не
ломается, следующий запуск просто скачивает архив заново.

---

## Тесты

```bash
cargo test
```

Покрытие:

- парсинг корректных, malformed и TEI-подобных XML;
- отсутствующие поля → `—`;
- сиглы / CTH / editor / date (включая Unicode);
- генерация HTML и экранирование;
- пути `~/Downloads`;
- сетевые ошибки, битый ZIP, пустой архив;
- большие и крошечные XML-записи.

Опциональный stress-тест на полном корпусе: положите ZIP в  
`fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip`.

---

## Universal Binary (macOS)

Минимальная версия: **macOS 13.0**.  
`MACOSX_DEPLOYMENT_TARGET=13.0`.

### Вручную

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
export MACOSX_DEPLOYMENT_TARGET=13.0

cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

lipo -create -output aruna-universal \
  target/aarch64-apple-darwin/release/aruna \
  target/x86_64-apple-darwin/release/aruna

lipo -info aruna-universal
# Architectures in the fat file: aruna-universal are: x86_64 arm64
```

### Одной командой → `Aruna.app`

```bash
chmod +x build_app.sh
./build_app.sh
```

Скрипт:

- собирает `aarch64-apple-darwin` + `x86_64-apple-darwin`;
- склеивает Universal Binary через `lipo`;
- собирает `icon.png` → `.icns` со всеми десятью представлениями (`sips` + `iconutil`);
- собирает `Aruna.app` (`Contents/MacOS`, `Contents/Resources`, `Info.plist`);
- выставляет `CFBundleName = Aruna`, `CFBundleIdentifier = com.sergeyssimonov.aruna`,  
  `LSMinimumSystemVersion = 13.0`, `CFBundleIconFile = AppIcon`;
- ad-hoc `codesign`.

Результат: `./Aruna.app` — запуск двойным кликом или:

```bash
open Aruna.app
# / «Aruna.app/Contents/MacOS/Aruna»
```

---

## Иконка

`icon.png` — глиняная табличка с клинописным знаком: 3D-рендер, вырезанный из
фона по измеренному силуэту, 1024×1024, sRGB, с альфа-каналом.

Разложен по сетке Apple: 824 px изображения по центру холста 1024 (поля по 100),
собственная тень выходит за силуэт на 19 px вбок и 29 px вниз — те же пропорции,
что у системных иконок macOS.

`build_app.sh` собирает из него `.icns` и проверяет, что на выходе ровно десять
представлений: 16, 32, 128, 256, 512 pt, каждое с `@2x`. Без `@2x`-половины
Retina-экран показывает растянутую копию низкого разрешения.

---

## Стратегия производительности

Единая на весь репозиторий — [../PERFORMANCE.md](../PERFORMANCE.md): принципы
в порядке приоритета, порядок замеров и уже принятые решения. Кратко:
корректность и читаемость важнее скорости, оптимизация без цифр до/после
подлежит откату, `unsafe` — только на границе FFI / WASM ABI.

Замеры: `cargo run --release --example bench_parse -- fixtures/…zip`.

## Структура проекта

```text
Aruna/
├── Cargo.toml          # release: LTO, codegen-units=1, strip, opt-level=3
├── src/
│   ├── main.rs         # CLI, zero args
│   ├── lib.rs          # pipeline
│   ├── download.rs     # Zenodo HTTPS
│   ├── archive.rs      # ZIP → records
│   ├── parse.rs        # heuristic AOxml / TEI parser
│   ├── xml_scan.rs     # tag / attribute scanners
│   ├── html.rs         # Scandinavian HTML
│   ├── paths.rs        # ~/Downloads via dirs
│   └── error.rs        # thiserror
├── tests/integration.rs
├── icon.png
├── build_app.sh
├── .gitignore
└── README.md
```

---

## Release-профиль

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
opt-level = 3
```

Без `panic = "abort"`: паника обязана печатать, где она произошла. В основном
пути нет `unwrap()` — только `Result` + `?` / `map_err`.

---

## GitHub

Репозиторий: [https://github.com/sergeyssimonov-max/Aruna](https://github.com/sergeyssimonov-max/Aruna)

```bash
cd cli
git init
git add .
git commit -m "Initial release: Aruna TLHdig HTML inventory generator"
git branch -M main
git remote add origin https://github.com/sergeyssimonov-max/Aruna.git
git push -u origin main
```

---

## Лицензия

MIT

## macOS release (only)

```bash
bash scripts/make_release.sh   # Universal .app + UDIF DMG
```

CI / tags: [docs/AUTO_DMG.md](docs/AUTO_DMG.md)

```bash
git tag v2.0.1 && git push origin v2.0.1   # тег должен совпадать с version в cli/Cargo.toml — CI проверяет
```
