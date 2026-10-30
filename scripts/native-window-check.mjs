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
const fixtureBinary = process.argv[3];
assert.ok(fixtureBinary, 'Run through cargo test so the private notification fixture is available');
const scratch = process.env.TMPDIR ?? process.env.RUNNER_TEMP ?? join(root, '.build');
await mkdir(scratch, { recursive: true });
const temporary = await mkdtemp(join(scratch, 'hermes-native-check-'));
const downloadDir = join(temporary, 'approved-downloads');
let child;
let replacementFixture;
let page;
let downloads = 0;
let popups = 0;
let nativeDiagnostics = '';
const checks = [];
// Exercise the real pinned upstream helper, not a look-alike implementation.
const downloadHelper = stripTypeScriptTypes(await readFile(join(root, 'upstream/hermes-agent/apps/desktop/src/lib/download.ts'), 'utf8'));
const fixtureHandler = (request, response) => {
  if (request.url === '/notification-worker.js') {
    response.writeHead(200, { 'Content-Type': 'text/javascript' });
    response.end(`self.addEventListener('install', () => self.skipWaiting());
      self.addEventListener('message', event => event.waitUntil((async () => {
        let result, reason;
        try { await self.registration.showNotification('WORKER_SELF_MUST_NOT_DELIVER'); result='resolved'; }
        catch (error) { result=error.name; reason=error.message; }
        event.ports[0].postMessage({workerAlive:true, result, reason, permission:Notification.permission, callable:typeof self.registration.showNotification});
      })()));`);
  } else if (request.url === '/download') {
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
      <button id="blob" onclick="fixtureDownloadBlob(new Blob(['BLOB_FIXTURE_BYTES'],{type:'application/octet-stream'}),'blob-proof.txt')">Export blob</button>
      <button id="notification-permission" onclick="window.fixturePermission='pending'; window.fixturePermissions=[]; Notification.requestPermission().then(value => {window.fixturePermissions[0]=value; window.fixturePermission=value})">Request notification permission</button>`);
  }
};
const fixture = createServer(fixtureHandler);
const alternateFixture = createServer(fixtureHandler);
const listen = (server, host = '127.0.0.1') => new Promise(resolve => server.listen(0, host, resolve));
async function notificationEvents() {
  const text = await readFile(join(temporary, 'notification-events.jsonl'), 'utf8');
  return text.slice(0, text.lastIndexOf('\n') + 1).split('\n').filter(Boolean).map(line => JSON.parse(line));
}
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
  // WebKit may reuse worker processes by site, not port. Keep the denied and granted
  // fixtures on distinct loopback sites so the worker cannot inherit the other page's cache.
  await listen(alternateFixture, '127.0.0.2');
  const origin = `http://127.0.0.1:${fixture.address().port}`;
  const alternateOrigin = `http://127.0.0.2:${alternateFixture.address().port}`;
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
  child = spawn('dbus-run-session', ['--', 'sh', '-c', '"$1" >/dev/null 2>&1 & "$2" --exact notification_service_fixture --ignored --nocapture >&2 & shift 2; exec "$@"', 'native-check', windowManager, fixtureBinary, binary, '--url', origin], {
    cwd: temporary, detached: true, stdio: ['ignore', 'ignore', 'pipe'],
    env: {
      PATH: process.env.PATH, DISPLAY: process.env.DISPLAY, XAUTHORITY: process.env.XAUTHORITY, LANG: 'C.UTF-8',
      HOME: join(temporary, 'home'), TMPDIR: temporary, GDK_BACKEND: 'x11',
      XDG_CONFIG_HOME: join(temporary, 'config'), XDG_DATA_HOME: join(temporary, 'data'),
      XDG_CACHE_HOME: join(temporary, 'cache'), XDG_STATE_HOME: join(temporary, 'state'),
      XDG_RUNTIME_DIR: join(temporary, 'runtime'),
      NOTIFICATION_FIXTURE_DIR: temporary,
      WEBKIT_INSPECTOR_HTTP_SERVER: `127.0.0.1:${inspectorPort}`,
      WEBKIT_INSPECTOR_SERVER: `127.0.0.1:${socketPort}`,
    },
  });
  child.stderr.on('data', bytes => { nativeDiagnostics = (nativeDiagnostics + bytes.toString()).slice(-8000); });
  await until('private notification service', async () => {
    try { return Boolean(JSON.parse(await readFile(join(temporary, 'notification-ready.json'), 'utf8')).address); }
    catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  });
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

  assert.equal(await page.evaluate("typeof Notification"), 'function');
  // Completion notifications ask programmatically, not from a special test-only gesture.
  await page.evaluate("window.fixturePermission='pending'; Notification.requestPermission().then(value => window.fixturePermission=value); true");
  await until('engine enforces user activation', () => page.evaluate("window.fixturePermission === 'denied'"));
  assert.equal(await page.evaluate('Notification.permission'), 'default', 'lack of a gesture must not be treated as the user blocking permission');
  assert.equal(Boolean(windowNamed('Allow desktop notifications')), false);
  await page.evaluate("window.pagePermissionHookCalled=false; window.realPermissionRequest=Notification.requestPermission; Notification.requestPermission=()=>{window.pagePermissionHookCalled=true; return Promise.resolve('granted')}; true");
  key(windowNamed('Hermes Desktop Rust'), 'ctrl+alt+n');
  await until('native notification consent', async () => {
    const permission = await page.evaluate('Notification.permission');
    assert.equal(permission, 'default', 'Notification permission must wait for native consent, not silently deny or allow');
    return Boolean(windowNamed('Allow desktop notifications'));
  });
  key(windowNamed('Allow desktop notifications'), 'Escape');
  await until('notification refusal reaches the page', () => page.evaluate("Notification.permission === 'denied'"));
  assert.equal(await page.evaluate('Notification.permission'), 'denied');
  assert.equal(await page.evaluate('window.pagePermissionHookCalled'), false, 'native activation must not execute a page replacement');
  await page.evaluate('Notification.requestPermission=window.realPermissionRequest; true');
  assert.deepEqual(await notificationEvents(), []);
  checks.push('notification permission waits for native consent and respects refusal');

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

  // Use the ordinary bundled connection form to open a fresh origin, not a native grant backdoor.
  page.close();
  page = null; // WebKit permits only one inspector connection per target.
  let launcher;
  for (const target of await targets(inspector)) {
    const candidate = await inspect(target);
    if (await candidate.evaluate("Boolean(document.querySelector('#remote-form'))")) {
      launcher = candidate;
      break;
    }
    candidate.close();
  }
  assert.ok(launcher);
  await launcher.evaluate(`document.querySelector('#remote-details').open=true; document.querySelector('#url').value=${JSON.stringify(alternateOrigin)}; document.querySelector('#remote-form button').focus(); true`);
  key(windowNamed('Hermes Desktop Rust.*Connect'), 'Return');
  launcher.close();
  await until('second notification workspace', async () => {
    for (const target of await targets(inspector)) {
      const candidate = await inspect(target);
      if (await candidate.evaluate(`location.origin === ${JSON.stringify(alternateOrigin)}`)) {
        page = candidate;
        return true;
      }
      candidate.close();
    }
    return false;
  });
  const notificationWindow = windowNamed('Hermes Desktop Rust');
  await page.evaluate("document.querySelector('#notification-permission').focus(); true");
  key(notificationWindow, 'Return');
  await until('second origin needs native consent', () => Boolean(windowNamed('Allow desktop notifications')));
  // Each request consumes transient activation in WebKit's Notification::requestPermission.
  // Give the concurrent request a separate inspector gesture while the native dialog is pending;
  // this drives the real engine request, not a permission or consent override.
  await page.call('Runtime.evaluate', {
    expression: 'Notification.requestPermission().then(value => {window.fixturePermissions[1]=value}); true',
    emulateUserGesture: true,
  });
  key(windowNamed('Allow desktop notifications'), 'Return');
  await until('notification permission granted', () => page.evaluate("window.fixturePermission === 'granted'"));
  await until('both permission requests resolved', () => page.evaluate('window.fixturePermissions[0] && window.fixturePermissions[1]'));
  assert.deepEqual(await page.evaluate('window.fixturePermissions'), ['granted', 'granted'], 'concurrent requests receive the real decision, not a busy denial');
  await page.evaluate("window.fixtureNotification = new Notification('Native delivery fixture', {body:'NOTIFICATION_FIXTURE_BYTES <b>not markup</b>'}); true");
  await until('notification reaches the private OS service', async () => (await notificationEvents()).some(event => event.event === 'notify'));
  const delivered = (await notificationEvents()).find(event => event.event === 'notify');
  assert.equal(delivered.app, 'hermes-desktop-rust'); // The system engine identifies the executable.
  assert.ok(delivered.title.includes('Native delivery fixture'));
  assert.equal(delivered.body, 'NOTIFICATION_FIXTURE_BYTES &lt;b&gt;not markup&lt;/b&gt;');
  checks.push('consented WebKit notification reaches the private OS bus with literal text');
  await page.evaluate('window.notificationClicked=false; window.fixtureNotification.onclick=()=>{window.notificationClicked=true}; true');
  const { address: notificationBus } = JSON.parse(await readFile(join(temporary, 'notification-ready.json'), 'utf8'));
  const notificationCall = (method, ...args) => execFileSync('gdbus', ['call', '--address', notificationBus, '--dest', 'org.freedesktop.Notifications', '--object-path', '/org/freedesktop/Notifications', '--method', `org.freedesktop.Notifications.${method}`, ...args], { encoding: 'utf8', timeout: 5000 });
  notificationCall('FixtureClick', String(delivered.id));
  await until('OS click reaches the browser notification', () => page.evaluate('window.notificationClicked'));
  await until('clicked notification closes at the OS', async () => (await notificationEvents()).some(event => event.event === 'close' && event.id === delivered.id));
  checks.push('OS click reaches the browser and closes the native notification');

  // Create the worker before revocation, and await activation rather than racing ready's
  // "has active worker" condition. Revocation must also protect an existing registration.
  assert.equal(await page.evaluate(`(async () => {
    if (!navigator.serviceWorker) throw new Error('Ordinary service workers must remain available');
    const registration = window.fixtureWorker = await navigator.serviceWorker.register('/notification-worker.js');
    await navigator.serviceWorker.ready;
    const worker = registration.active;
    if (worker.state !== 'activated') await new Promise(resolve => {
      const changed = () => { if (worker.state === 'activated') { worker.removeEventListener('statechange', changed); resolve(); } };
      worker.addEventListener('statechange', changed);
    });
    return worker.state;
  })()`), 'activated');

  key(notificationWindow, 'ctrl+alt+m');
  await page.evaluate("window.suppressedClosed=false; const n=new Notification('MUST_NOT_DELIVER'); n.onclose=()=>{window.suppressedClosed=true}; true");
  await until('stopped notification is closed without delivery', () => page.evaluate('window.suppressedClosed'));
  assert.equal((await notificationEvents()).filter(event => event.event === 'notify').length, 1);
  assert.equal(await page.evaluate('Notification.permission'), 'granted', 'stopping host delivery does not spoof engine permission');
  checks.push('stopping notifications suppresses delivery without falsifying browser permission');

  key(notificationWindow, 'ctrl+alt+n');
  await until('resuming asks for explicit consent', () => Boolean(windowNamed('Allow desktop notifications')));
  key(windowNamed('Allow desktop notifications'), 'Return');
  notificationCall('FixtureSetMarkup', 'false');
  await page.evaluate("window.fixtureResumed = new Notification('RESUMED_FIXTURE', {body:'<b>literal without markup</b>'}); true");
  await until('resumed delivery', async () => (await notificationEvents()).some(event => event.title === 'RESUMED_FIXTURE'));
  const resumed = (await notificationEvents()).find(event => event.title === 'RESUMED_FIXTURE');
  assert.equal(resumed.body, '<b>literal without markup</b>', 'plain-text daemons must not display entity escapes');
  await page.evaluate('window.fixtureResumed.close(); true');
  await until('browser close reaches the OS', async () => (await notificationEvents()).some(event => event.event === 'close' && event.id === resumed.id));
  checks.push('native consent re-enables delivery; browser close releases the OS notification');

  // Numeric IDs belong to a particular daemon, not its reusable well-known bus name.
  await page.evaluate("window.staleClicked=false; window.ownerOldClick=new Notification('OWNER_OLD_CLICK'); ownerOldClick.onclick=()=>{window.staleClicked=true}; window.ownerOldClose=new Notification('OWNER_OLD_CLOSE'); true");
  await until('old daemon owns both notifications', async () => {
    const events = await notificationEvents();
    return ['OWNER_OLD_CLICK', 'OWNER_OLD_CLOSE'].every(title => events.some(event => event.title === title));
  });
  const oldNotes = (await notificationEvents()).filter(event => ['OWNER_OLD_CLICK', 'OWNER_OLD_CLOSE'].includes(event.title)).sort((a, b) => a.id - b.id);
  replacementFixture = spawn(fixtureBinary, ['--exact', 'notification_service_fixture', '--ignored', '--nocapture'], {
    cwd: temporary, stdio: ['ignore', 'ignore', 'pipe'],
    env: {
      PATH: process.env.PATH, LANG: 'C.UTF-8', HOME: join(temporary, 'home'), TMPDIR: temporary,
      DBUS_SESSION_BUS_ADDRESS: notificationBus, NOTIFICATION_FIXTURE_DIR: temporary,
      NOTIFICATION_FIXTURE_START_ID: String(oldNotes[0].id),
    },
  });
  replacementFixture.stderr.on('data', bytes => { nativeDiagnostics = (nativeDiagnostics + bytes.toString()).slice(-8000); });
  let replacement;
  await until('replacement daemon owns the name', async () => {
    if (replacementFixture.exitCode !== null) throw new Error(`Replacement fixture exited: ${replacementFixture.exitCode}`);
    try { replacement = JSON.parse(await readFile(join(temporary, 'notification-replacement-ready.json'), 'utf8')); return true; }
    catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  });
  assert.equal(replacement.address, notificationBus);
  assert.ok(replacement.owner);
  for (const old of oldNotes) {
    assert.notEqual(replacement.owner, old.owner);
    const reply = notificationCall('Notify', 'other-fixture-app', '0', '', 'Unrelated fixture notification', '', "['default', 'Open']", '{}', '2000');
    assert.equal(Number(reply.match(/uint32 (\d+)/)?.[1]), old.id, 'new daemon must really reuse the old IDs');
  }
  await page.evaluate("window.currentClicked=false; window.ownerCurrentClick=new Notification('OWNER_CURRENT_CLICK'); ownerCurrentClick.onclick=()=>{window.currentClicked=true}; window.ownerCurrentClose=new Notification('OWNER_CURRENT_CLOSE'); true");
  await until('new notifications reach the replacement daemon', async () => {
    const events = await notificationEvents();
    return ['OWNER_CURRENT_CLICK', 'OWNER_CURRENT_CLOSE'].every(title => events.some(event => event.title === title && event.owner === replacement.owner));
  });
  const currentNotes = (await notificationEvents()).filter(event => ['OWNER_CURRENT_CLICK', 'OWNER_CURRENT_CLOSE'].includes(event.title));
  notificationCall('FixtureClick', String(oldNotes.find(event => event.title === 'OWNER_OLD_CLICK').id));
  notificationCall('FixtureClick', String(currentNotes.find(event => event.title === 'OWNER_CURRENT_CLICK').id));
  // A valid later signal from the same sender is a processing barrier, not a timed negative check.
  await until('current daemon click still works', () => page.evaluate('window.currentClicked'));
  const staleClick = await page.evaluate('window.staleClicked');
  await page.evaluate('window.ownerOldClose.close(); window.ownerCurrentClose.close(); true');
  const currentClose = currentNotes.find(event => event.title === 'OWNER_CURRENT_CLOSE');
  await until('current daemon close still works', async () => (await notificationEvents()).some(event => event.event === 'close' && event.owner === replacement.owner && event.id === currentClose.id));
  const wrongClose = (await notificationEvents()).some(event => event.event === 'close' && event.owner === replacement.owner && oldNotes.some(old => old.id === event.id));
  assert.deepEqual({ staleClick, wrongClose }, { staleClick: false, wrongClose: false }, 'old handlers must not act on a replacement daemon\'s reused IDs');
  checks.push('daemon replacement cannot reroute old clicks or close unrelated notifications');

  key(notificationWindow, 'ctrl+alt+m');
  const workerResult = await page.evaluate(`(async () => {
    const registration = window.fixtureWorker;
    try {
      let fromPage;
      try { await registration.showNotification('SERVICE_WORKER_MUST_NOT_DELIVER'); fromPage='resolved'; }
      catch (error) { fromPage=error.name; }
      const channel = new MessageChannel();
      const fromWorker = await new Promise(resolve => {
        channel.port1.onmessage = event => { channel.port1.close(); resolve(event.data); };
        registration.active.postMessage('notification-probe', [channel.port2]);
      });
      return {fromPage, fromWorker};
    } finally { await registration.unregister(); }
  })()`);
  assert.equal(workerResult.fromWorker.workerAlive, true, 'worker execution and messaging are preserved');
  assert.equal(workerResult.fromPage, 'resolved', 'the real persistent API remains present');
  assert.equal(workerResult.fromWorker.callable, 'function');
  assert.equal(workerResult.fromWorker.permission, 'granted', 'worker permissions are not spoofed');
  assert.equal(workerResult.fromWorker.result, 'resolved', workerResult.fromWorker.reason);
  assert.equal((await notificationEvents()).some(event => ['SERVICE_WORKER_MUST_NOT_DELIVER', 'WORKER_SELF_MUST_NOT_DELIVER'].includes(event.title)), false, `service-worker delivery bypassed consent (${JSON.stringify(workerResult)})`);
  checks.push(`worker notifications cannot bypass stopped delivery; worker messaging works (${JSON.stringify(workerResult)})`);

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
  if (replacementFixture && replacementFixture.exitCode === null && replacementFixture.signalCode === null) {
    replacementFixture.kill('SIGTERM');
    await Promise.race([new Promise(resolve => replacementFixture.once('exit', resolve)), delay(3000)]);
    if (replacementFixture.exitCode === null && replacementFixture.signalCode === null) replacementFixture.kill('SIGKILL');
  }
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
  await new Promise(resolve => alternateFixture.close(resolve));
  await rm(temporary, { recursive: true, force: true });
}
