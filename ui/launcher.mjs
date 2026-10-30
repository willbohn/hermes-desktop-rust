import { createActions } from './actions.mjs';

const byId = id => document.getElementById(id);
const invoke = (...args) => window.__TAURI__.core.invoke(...args);
const actions = createActions(invoke, state => {
  document.querySelectorAll('button, input').forEach(control => { control.disabled = state.busy; });
  byId('feedback').setAttribute('aria-busy', String(state.busy));
  byId('status').textContent = state.message;
  byId('error').hidden = !state.error;
  if (state.error) {
    byId('error-title').textContent = state.error.message || 'The workspace could not be opened.';
    byId('recovery').textContent = state.error.recovery || 'Try again, or run hermes doctor in a terminal. No settings were changed.';
  }
});
const localRequest = () => ({ mode: 'local', profile: byId('profile').value.trim() || null });
byId('local-form').addEventListener('submit', event => { event.preventDefault(); actions.connect(localRequest()); });
byId('remote-form').addEventListener('submit', event => { event.preventDefault(); actions.connect({ mode: 'remote', url: byId('url').value.trim() }); });
byId('build').addEventListener('click', () => actions.build(localRequest()));

try {
  const options = await invoke('launch_options');
  byId('profile').value = options.profile || '';
  if (options.url) {
    byId('remote-details').open = true;
    byId('url').value = options.url;
  }
  if (options.auto_connect) {
    actions.connect(options.url ? { mode: 'remote', url: options.url } : localRequest());
  }
} catch {
  byId('error').hidden = false;
  byId('error-title').textContent = 'The native host is unavailable.';
  byId('recovery').textContent = 'Open this launcher through the Hermes Desktop Rust executable, not as a web page.';
}
