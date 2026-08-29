<script lang="ts">
  /**
   * The window, while there is no interface to put in it.
   *
   * Aruna is a command-line program: it downloads the TLHdig corpus, normalises
   * it and writes the package with its inventory. The desktop shell exists —
   * window, permissions, the bridge — but not one command is registered on the
   * Rust side yet, so there is nothing here to drive.
   *
   * **The screen is deliberately disposable.** It is drawn again through
   * `/design` on every maintenance pass and rebuilt by hand from what comes
   * back, so that the design tool and this stack — Svelte 5 and Vite without
   * SvelteKit — are known to still work together. What survives each round is
   * not the markup but the contract: this heading, this sentence, and one probe
   * button.
   *
   * This round reads as a document rather than a splash: the column is
   * left-aligned under a hairline, which is the one place `--border` earns its
   * keep on this screen. A window that tells the reader to go back to the
   * terminal should not announce itself like a title card.
   *
   * **The button is not a feature.** It is the one place in this repository
   * where a state change is proven to reach the DOM in the WKWebView macOS 13
   * ships and never updates. That is why `build.target` is pinned at `safari16`
   * and why §4.3 of the specification says a dev-mode run proves nothing about
   * the floor: only the built application, driven by `e2e/smoke.e2e.ts`, does —
   * and it does it by clicking this and reading the text back. The scenario is
   * the measuring instrument and does not move; the screen around it may.
   */
  let clicks: number = $state(0)
</script>

<main>
  <h1>Aruna</h1>
  <hr />
  <p>
    The desktop interface is not implemented yet. The corpus package and its inventory are built by
    the command-line program.
  </p>
  <button type="button" class="probe" onclick={() => (clicks += 1)}>
    Engine probe: {clicks}
  </button>
</main>
