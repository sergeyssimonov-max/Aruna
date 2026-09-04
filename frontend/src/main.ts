// The WebDriver bridge, and only in an E2E build: `VITE_E2E` is unset in every
// other one, so the import is not reachable and the bundler drops it — which is
// half of what keeps the contour out of a release.
//
// It sits above the static imports because it has to run before the app mounts,
// and reads oddly for the same reason: static imports are hoisted, so `./app.css`
// and `App.svelte` are evaluated before this line whatever its position, while
// the statements below it — `mount` among them — are not. Moving it under them
// would mount the application before the bridge patches the globals it needs.
if (import.meta.env.VITE_E2E) {
  await import('@wdio/tauri-plugin')
}

import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

mount(App, {
  target: document.getElementById('app')!,
})
