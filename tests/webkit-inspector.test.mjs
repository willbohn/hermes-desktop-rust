// Control the transport handshake, not the WebKit behavior exercised by the native suite.
import test from 'node:test';
import assert from 'node:assert/strict';
import * as webkit from '../scripts/webkit-inspector.mjs';

test('discovery skips pages without a loaded document, not loaded pages with a blank title', async t => {
  const row = (id, title, url) => `<tr><td class="data"><div class="targetname">${title}</div><div class="targeturl">${url}</div></td><td><input onclick="window.open('/socket/1/${id}/WebPage')"></td></tr>`;
  const html = `<table>${row(1, '', 'about:blank')}${row(2, '', '')}${row(3, 'about:blank', 'http://127.0.0.1/fixture')}${row(4, '', 'tauri://localhost/index.html')}</table>`;
  t.mock.method(globalThis, 'fetch', async () => new Response(html));
  assert.deepEqual(await webkit.targets('http://inspector.test'), [
    'ws://inspector.test/socket/1/3/WebPage',
    'ws://inspector.test/socket/1/4/WebPage',
  ]);
});

function controlledSocket(t) {
  const original = globalThis.WebSocket;
  const sockets = [];
  globalThis.WebSocket = class extends EventTarget {
    static CLOSED = 3;
    readyState = 1;
    closeStarted = Promise.withResolvers();
    constructor() {
      super();
      sockets.push(this);
      queueMicrotask(() => this.onmessage({ data: JSON.stringify({
        method: 'Target.targetCreated', params: { targetInfo: { targetId: 'fixture' } },
      }) }));
    }
    close() { this.readyState = 2; this.closeStarted.resolve(); }
    finishClose() { this.readyState = 3; this.dispatchEvent(new Event('close')); }
  };
  t.after(() => { sockets.forEach(socket => socket.finishClose()); globalThis.WebSocket = original; });
  return sockets;
}

test('closing waits for the transport to release the exclusive inspector connection', async t => {
  const sockets = controlledSocket(t);
  const page = await webkit.inspect('ws://fixture');
  let finished = false;
  const closing = Promise.resolve(page.close()).then(() => { finished = true; });
  await Promise.resolve();
  assert.equal(finished, false, 'reconnecting now would overlap the existing WebKit inspector');
  sockets[0].finishClose();
  await closing;
  assert.equal(finished, true);
});

test('a session retains one connection per target until all owned transports close', async t => {
  const sockets = controlledSocket(t);
  const session = webkit.createInspectorSession();
  const pending = session.inspect('ws://workspace');
  assert.equal(session.inspect('ws://workspace'), pending, 'concurrent discovery must share attachment');
  const workspace = await pending;
  await session.inspect('ws://launcher');
  assert.equal(await session.inspect('ws://workspace'), workspace, 'returning to a page must not reconnect');
  assert.equal(sockets.length, 2);
  assert.ok(sockets.every(socket => socket.readyState === 1));
  let finished = false;
  const closing = session.close().then(() => { finished = true; });
  await Promise.all(sockets.map(socket => socket.closeStarted.promise));
  assert.equal(finished, false);
  await assert.rejects(session.inspect('ws://late'), /closed/);
  sockets.forEach(socket => socket.finishClose());
  await closing;
  assert.equal(finished, true);
  assert.equal(sockets.length, 2, 'teardown must not create another connection');
});
