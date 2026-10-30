// Control the transport handshake, not the WebKit behavior exercised by the native suite.
import test from 'node:test';
import assert from 'node:assert/strict';
import { inspect } from '../scripts/webkit-inspector.mjs';

function controlledSocket(t) {
  const original = globalThis.WebSocket;
  let socket;
  globalThis.WebSocket = class extends EventTarget {
    static CLOSED = 3;
    readyState = 1;
    constructor() {
      super();
      socket = this;
      queueMicrotask(() => this.onmessage({ data: JSON.stringify({
        method: 'Target.targetCreated', params: { targetInfo: { targetId: 'fixture' } },
      }) }));
    }
    close() { this.readyState = 2; }
    finishClose() { this.readyState = 3; this.dispatchEvent(new Event('close')); }
  };
  t.after(() => { socket?.finishClose(); globalThis.WebSocket = original; });
  return () => socket;
}

test('closing waits for the transport to release the exclusive inspector connection', async t => {
  const socket = controlledSocket(t);
  const page = await inspect('ws://fixture');
  let finished = false;
  const closing = Promise.resolve(page.close()).then(() => { finished = true; });
  await Promise.resolve();
  assert.equal(finished, false, 'reconnecting now would overlap the existing WebKit inspector');
  socket().finishClose();
  await closing;
  assert.equal(finished, true);
});
