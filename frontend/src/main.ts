if (import.meta.env.VITE_E2E) {
  await import('@wdio/tauri-plugin')
}

import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
