import js from '@eslint/js'
import ts from 'typescript-eslint'
import svelte from 'eslint-plugin-svelte'
import prettier from 'eslint-config-prettier'
import svelteParser from 'svelte-eslint-parser'

export default ts.config(
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs['flat/recommended'],
  prettier,
  ...svelte.configs['flat/prettier'],
  {
    files: ['src/**/*.ts'],
    extends: [...ts.configs.recommendedTypeChecked],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  {
    files: ['**/*.svelte'],
    extends: [...ts.configs.recommendedTypeChecked],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: ts.parser,
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
        extraFileExtensions: ['.svelte'],
      },
    },
  },
  /**
   * The exported inventory's markup, which is not a user interface.
   *
   * `src/inventory/*.svelte` is rendered once at build time by
   * `build/inventory.ts` and written into `cli/src/generated/`, where the Rust
   * crate compiles it in. Nothing here is ever mounted, hydrated or shown to
   * anyone in a browser: what a reader's browser runs is `filter.ts`. Two rules
   * that are right for a window are wrong for that, and both are disabled here
   * rather than worked around in the components.
   */
  {
    files: ['src/inventory/**/*.svelte'],
    rules: {
      /*
       * `{@html}` is how a slot receives markup rather than text. At build time
       * every one of them is handed a placeholder (`@@ROWS@@`); at run time the
       * Rust side substitutes markup it assembled from these same components,
       * out of text it had already put through `escape_html`. There is no path
       * from anything a reader types to any of them.
       */
      'svelte/no-at-html-tags': 'off',
      /*
       * `<head>` and `<body>` are literal here because this component *is* the
       * whole document, from `<html>` to `</html>`. `<svelte:head>` is the
       * right answer for a page an application owns — it returns its content
       * in a separate `head` field for a server to place — and the wrong one
       * for a file that has to come out of `render()` whole.
       */
      'svelte/no-raw-special-elements': 'off',
    },
  },
  { ignores: ['dist/', 'node_modules/', 'coverage/'] },
)
