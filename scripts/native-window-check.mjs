#!/usr/bin/env node
// Actual Linux/WebKit test against an explicitly labelled HTTP fixture, never a live Hermes home.
// Run under xvfb-run -a; xdotool and a window manager use only that isolated display.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { stripTypeScriptTypes } from 'node:module';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { inspect, targets } from './webkit-inspector.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary = resolve(process.argv[2] ?? join(root, 'target/debug/hermes-desktop-rust'));
const scratch = process.env.TMPDIR ?? process.env.RUNNER_TEMP ?? join(root, '.build');
await mkdir(scratch, { recursive: true });
const temporary = await mkdtemp(join(scratch, 'hermes-native-check-'));
const downloadDir = join(temporary, 'approved-downloads');
let child;
let page;
let downloads = 0;
let popups = 0;
let nativeDiagnostics = '';
const checks = [];
// Exercise the real pinned upstream helper, not a look-alike implementation.
const downloadHelper = stripTypeScriptTypes(await readFile(join(root, 'upstream/hermes-agent/apps/desktop/src/lib/download.ts'), 'utf8'));
const fixture = createServer((request, response) => {
  if (request.url === '/download') {
    downloads++;
    if (!request.headers.cookie?.includes('fixture-session=approved')) {
      response.writeHead(401).end('Fixture authentication required');
      return;
    }
    response.writeHead(200, {
      'Content-Type': 'application/octet-stream',
      'Content-Disposition': 'attachment; filename="cookie-proof.txt"',
    }).end('AUTHENTICATED_FIXTURE_BYTES');
  } else if (request.url === '/download-helper.js') {
    response.writeHead(200, { 'Content-Type': 'text/javascript' }).end(downloadHelper);
  } else if (request.url === '/popup') {
    popups++;
    response.writeHead(200, { 'Content-Type': 'text/html' }).end('<p>Popup fixture</p>');
  } else {
    response.writeHead(200, {
      'Content-Type': 'text/html',
      'Set-Cookie': 'fixture-session=approved; HttpOnly; SameSite=Strict; Path=/',
    }).end(`<!doctype html><title>Native boundary fixture</title>
      <h1>Test fixture — not a Hermes session</h1>
      <a id="popup" href="/popup" target="_blank">Open fixture popup</a>
      <script type="module">import { downloadBlob } from '/download-helper.js'; window.fixtureDownloadBlob = downloadBlob;</script>
      <button id="blob" onclick="fixtureDownloadBlob(new Blob(['BLOB_FIXTURE_BYTES'],{type:'application/octet-stream'}),'blob-proof.txt')">Export blob</button>`);
  }
});
const listen = server => new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
async function freePort() {
  const server = createServer();
  await listen(server);
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}
async function until(label, condition, timeout = 10000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (await condition()) return;
    await delay(80);
  }
  throw new Error(`Timed out: ${label}`);
}
const xdo = (...args) => execFileSync('xdotool', args, { encoding: 'utf8', timeout: 5000 }).trim();
function windowNamed(name) {
  try { return xdo('search', '--onlyvisible', '--name', name).split('\n').at(-1); }
  catch (error) { if (error.status === 1) return null; throw error; }
}
function key(window, keys) {
  xdo('windowfocus', '--sync', window);
  xdo('key', '--clearmodifiers', keys);
}
try {
  await mkdir(downloadDir);
  for (const folder of ['home', 'config', 'data', 'cache', 'state', 'runtime']) {
    await mkdir(join(temporary, folder), { mode: 0o700 });
  }
  // OS preference only: this deliberately does NOT grant native download permission.
  // The first download must still be cancelled, despite a valid default directory.
  await writeFile(join(temporary, 'config/user-dirs.dirs'), `XDG_DOWNLOAD_DIR="${downloadDir}"\n`);
  await listen(fixture);
  const origin = `http://127.0.0.1:${fixture.address().port}`;
  const inspectorPort = await freePort();
  let socketPort = await freePort();
  while (socketPort === inspectorPort) socketPort = await freePort();
  const inspector = `http://127.0.0.1:${inspectorPort}`;
  // GTK's modal focus/default actions need a window manager as well as a session bus.
  // Start both inside the isolated display/home, never on the user's desktop or bus.
  const windowManager = ['openbox', 'kwin_x11'].find(command => {
    try { execFileSync('which', [command], { stdio: 'ignore' }); return true; } catch { return false; }
  });
  assert.ok(windowManager, 'Install openbox for the isolated native test');
  child = spawn('dbus-run-session', ['--', 'sh', '-c', '"$1" >/dev/null 2>&1 & shift; exec "$@"', 'native-check', windowManager, binary, '--url', origin], {
    cwd: temporary, detached: true, stdio: ['ignore', 'ignore', 'pipe'],
    env: {
      PATH: process.env.PATH, DISPLAY: process.env.DISPLAY, XAUTHORITY: process.env.XAUTHORITY, LANG: 'C.UTF-8',
      HOME: join(temporary, 'home'), TMPDIR: temporary, GDK_BACKEND: 'x11',
      XDG_CONFIG_HOME: join(temporary, 'config'), XDG_DATA_HOME: join(temporary, 'data'),
      XDG_CACHE_HOME: join(temporary, 'cache'), XDG_STATE_HOME: join(temporary, 'state'),
      XDG_RUNTIME_DIR: join(temporary, 'runtime'),
      WEBKIT_INSPECTOR_HTTP_SERVER: `127.0.0.1:${inspectorPort}`,
      WEBKIT_INSPECTOR_SERVER: `127.0.0.1:${socketPort}`,
    },
  });
  child.stderr.on('data', bytes => { nativeDiagnostics = (nativeDiagnostics + bytes.toString()).slice(-8000); });
  await until('workspace appears', async () => {
    if (child.exitCode !== null) throw new Error(`Native host exited: ${child.exitCode}`);
    let pages;
    try { pages = await targets(inspector); } catch { return false; }
    for (const target of pages) {
      const candidate = await inspect(target);
      if (await candidate.evaluate(`location.origin === ${JSON.stringify(origin)}`)) {
        page = candidate;
        return true;
      }
      candidate.close();
    }
    return false;
  });
  assert.equal(await page.evaluate(`(async () => {
    if (!window.__TAURI_INTERNALS__?.invoke) return 'not exposed';
    try { await window.__TAURI_INTERNALS__.invoke('launch_options'); return 'UNEXPECTED ACCESS'; }
    catch { return 'denied'; }
  })()`), 'denied');
  checks.push('remote native IPC denied');

  await page.evaluate("location.href='/download'; true");
  await until('download was requested', () => downloads === 1);
  await until('consent notice', () => Boolean(windowNamed('Allow workspace downloads')));
  assert.deepEqual((await readdir(temporary, { recursive: true })).filter(name => name.includes('cookie-proof')), []);
  assert.deepEqual(await readdir(downloadDir), []);
  checks.push('no file written before native consent');
  key(windowNamed('Allow workspace downloads'), 'Escape');
  await until('notice closed', () => !windowNamed('Allow workspace downloads'));

  const workspace = windowNamed('Hermes Desktop Rust');
  assert.ok(workspace);
  key(workspace, 'ctrl+shift+d');
  await until('native folder chooser', () => Boolean(windowNamed('Allow workspace downloads here')));
  const picker = windowNamed('Allow workspace downloads here');
  // The chooser starts in the OS download folder; consent is still a real native
  // Open action. No test-only IPC or automatic approval bypasses the picker.
  await delay(250);
  key(picker, 'alt+o');
  await until('folder chooser closed', () => !windowNamed('Allow workspace downloads here'));

  await page.evaluate("location.href='/download'; true");
  await until('authenticated bytes saved', async () => {
    try { return (await readFile(join(downloadDir, 'cookie-proof.txt'), 'utf8')) === 'AUTHENTICATED_FIXTURE_BYTES'; }
    catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  });
  checks.push('engine preserves authentication and saves only to chosen folder');
  await until('upstream download helper ready', () => page.evaluate("typeof fixtureDownloadBlob === 'function'"));
  await page.evaluate("document.querySelector('#blob').click(); true");
  await until('blob bytes saved', async () => {
    try { return (await readFile(join(downloadDir, 'blob-proof.txt'), 'utf8')) === 'BLOB_FIXTURE_BYTES'; }
    catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  });
  checks.push('upstream blob export helper saves through the native engine');

  // Popup policy includes the engine's user-gesture gate. A real keyboard
  // activation exercises it; a script-generated click is correctly suppressed.
  await page.evaluate("document.querySelector('#popup').focus(); true");
  key(workspace, 'Return');
  await until('same-origin popup', () => popups > 0);
  await delay(250);
  assert.equal(popups, 1);
  checks.push('same-origin popup loads exactly once');

  key(workspace, 'ctrl+shift+o');
  await until('connection launcher visible', () => Boolean(windowNamed('Hermes Desktop Rust.*Connect')));
  key(windowNamed('Hermes Desktop Rust.*Connect'), 'ctrl+w');
  await until('connection launcher closed', () => !windowNamed('Hermes Desktop Rust.*Connect'));
  key(workspace, 'ctrl+shift+o');
  await until('connection recovery still available', () => Boolean(windowNamed('Hermes Desktop Rust.*Connect')));
  checks.push('closing the connection launcher does not strand active workspaces');
  key(workspace, 'ctrl+q');
  await until('explicit quit exits the host', () => child.exitCode !== null);
  assert.equal(child.exitCode, 0);
  checks.push('explicit quit exits cleanly');
  console.log(JSON.stringify({ passed: checks.length, checks }));
} catch (error) {
  console.error(error.message);
  console.error('Downloads in isolated folder:', await readdir(downloadDir));
  console.error('Isolated native diagnostics:', nativeDiagnostics);
  await mkdir(join(root, 'artifacts'), { recursive: true });
  try { execFileSync('import', ['-window', 'root', join(root, 'artifacts/native-test-failure.png')], { timeout: 5000 }); } catch {}
  try { console.error('Visible test windows:', xdo('search', '--onlyvisible', '--name', '.')); } catch {}
  throw error;
} finally {
  page?.close();
  if (child?.pid) {
    // This process group contains only the test host/webviews; --url never invokes Hermes.
    // Reap ancillary fixture processes even after the native Quit action exits the host.
    try { process.kill(-child.pid, 'SIGTERM'); }
    catch (error) { if (error.code !== 'ESRCH') throw error; }
    if (child.exitCode === null && child.signalCode === null) {
      await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(3000)]);
    }
    if (child.exitCode === null && child.signalCode === null) process.kill(-child.pid, 'SIGKILL');
  }
  await new Promise(resolve => fixture.close(resolve));
  await rm(temporary, { recursive: true, force: true });
}
