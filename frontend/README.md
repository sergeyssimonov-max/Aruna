# frontend

Two things are built from this directory, and they have almost nothing to do
with each other.

**`src/inventory/`** — the client script and the three stylesheet sections that
the Rust CLI compiles into its binary and writes into the exported inventory.
This is the half that ships today. `pnpm build:inventory` builds it into
`../cli/src/generated/`, where the artifacts are committed; `tests/inventory-artifact.test.ts`
fails if what is committed is not what these sources now produce.

**`src/App.svelte` and the rest of `src/`** — the desktop window, which is still
the scaffold `create-vite` produced. Since 2026-08-30 it carries one real
screen, the Tauri shell in `../src-tauri` registers two commands
(`corpus_location` and `corpus_stats`), and what is still true is that nothing here reaches
the corpus. Read it as work in progress rather than as a second program.

The stack is Svelte 5, Vite and TypeScript, with pnpm as the only package
manager. **There is no SvelteKit**, and `tests/one-frontend-stack.test.ts` is
what keeps it that way — along with React, which was removed with the website on
2026-08-23.

## Commands

```bash
pnpm check          # svelte-check over the app, tsc over the configs and node tests
pnpm lint           # eslint
pnpm format:check   # prettier
pnpm test:unit      # vitest — node project for tests/, jsdom for src/
pnpm build          # the window, into dist/
pnpm build:inventory # the CLI's artifacts, into ../cli/src/generated/
pnpm test:e2e       # WebdriverIO inside a debug Tauri build
```

`pnpm dev` and `pnpm build` for the window itself are run from the repository
root, where Tauri drives them.

## Where the decisions are written down

| document                                                       | what it holds                                                                                            |
| -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| [`../README.md`](../README.md)                                 | what the project is today, and what it is not                                                            |
| [`../docs/FRONTEND-CONTRACT.md`](../docs/FRONTEND-CONTRACT.md) | the agreed stack, what the Rust core owes it, and what the removal of the React application took with it |
| [`../docs/PROJECT-SPEC.ru.md`](../docs/PROJECT-SPEC.ru.md)     | the normative specification: every component pinned, with a status                                       |
| [`../docs/TESTING.md`](../docs/TESTING.md)                     | the test profiles and the exact command for each                                                         |
| [`../docs/FONTS.md`](../docs/FONTS.md)                         | the cuneiform font stack, why it is duplicated, and what it costs                                        |
