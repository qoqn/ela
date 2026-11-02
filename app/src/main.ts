import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';

async function start() {
  const preview = import.meta.env.DEV && !('__TAURI_INTERNALS__' in window);
  if (preview) (await import('./dev/preview-backend')).install();
  const app = mount(App, { target: document.getElementById('app')! });
  if (preview) {
    const { workspace } = await import('./lib/sessions.svelte');
    await workspace.load();
    (await import('./dev/scenes')).loadScene(new URLSearchParams(location.search).get('scene'));
  }
  return app;
}

export default start();
