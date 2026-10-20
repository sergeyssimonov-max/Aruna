# frontend

Из этого каталога собираются две вещи, и между собой у них почти нет ничего
общего.

**`src/inventory/`** – клиентский скрипт и три раздела стилей, которые
консольная программа на Rust встраивает в свой бинарник и записывает в
выгружаемую опись. Это та половина, которая выходит сегодня. `pnpm build:inventory`
собирает ее в `../cli/src/generated/`, где артефакты лежат в репозитории.
`tests/inventory-artifact.test.ts` падает, если закоммиченное – не то, что
сейчас дают эти исходники.

**`src/App.svelte` и остальное в `src/`** – окно настольного приложения. С
2026-08-30 в нем один настоящий экран, а с 2026-09-04 именно это окно ставит
читатель. Оболочка Tauri в `../src-tauri` регистрирует шесть команд
(`corpus_location`, `corpus_stats`, `corpus_xml`, `open_inventory`,
`build_corpus`, `cancel_build`): окно читает пакет на диске, собирает его тем же
ядром, что и консольная программа, и открывает опись.

Стек – Svelte 5, Vite и TypeScript, единственный менеджер пакетов – pnpm.
**SvelteKit здесь нет**, и так это держит `tests/one-frontend-stack.test.ts`,
как и отсутствие React, удаленного вместе с сайтом 2026-08-23.

## Команды

```bash
pnpm check          # svelte-check over the app, tsc over the configs and node tests
pnpm lint           # eslint
pnpm format:check   # prettier
pnpm test:unit      # vitest – node project for tests/, jsdom for src/
pnpm build          # the window, into dist/
pnpm build:inventory # the CLI's artifacts, into ../cli/src/generated/
pnpm test:e2e       # WebdriverIO inside a debug Tauri build
```

`pnpm dev` и `pnpm build` для самого окна запускаются из корня репозитория, где
ими управляет Tauri.

## Где записаны решения

| документ                                                       | что в нем                                                                                         |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| [`../README.md`](../README.md)                                 | что проект собой представляет сегодня и чем не является                                           |
| [`../docs/FRONTEND-CONTRACT.md`](../docs/FRONTEND-CONTRACT.md) | согласованный стек, что ядро на Rust ему должно и что унесло с собой удаление приложения на React |
| [`../docs/PROJECT-SPEC.ru.md`](../docs/PROJECT-SPEC.ru.md)     | нормативная спецификация: каждый компонент закреплен и имеет статус                               |
| [`../docs/TESTING.md`](../docs/TESTING.md)                     | профили тестов и точная команда каждого                                                           |
| [`../docs/FONTS.md`](../docs/FONTS.md)                         | стек клинописных шрифтов, зачем он продублирован и чего стоит                                     |
