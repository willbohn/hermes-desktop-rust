// Test-only client for the system WebKit inspector. No npm dependencies.
// Enable WEBKIT_INSPECTOR_HTTP_SERVER only on loopback in an isolated smoke run.
import { writeFile } from 'node:fs/promises';
import { once } from 'node:events';

export async function targets(base = 'http://127.0.0.1:19222') {
  const html = await (await fetch(base)).text();
  return [...html.matchAll(/\/socket\/(\d+)\/(\d+)\/WebPage/g)].map(match => `${base.replace('http', 'ws')}${match[0]}`);
}

// Discovery revisits pages. Retain their exclusive connections instead of racing backend detach.
export function createInspectorSession() {
  const pages = new Map();
  let closed = false;
  return {
    inspect(target) {
      if (closed) return Promise.reject(new Error('Inspector session closed'));
      if (!pages.has(target)) pages.set(target, inspect(target));
      return pages.get(target);
    },
    async close() {
      closed = true;
      const opened = await Promise.allSettled(pages.values());
      await Promise.all(opened.filter(result => result.status === 'fulfilled').map(result => result.value.close()));
      pages.clear();
    },
  };
}

export async function inspect(target) {
  const socket = new WebSocket(target);
  let sequence = 0;
  const waiting = new Map();
  let acceptTarget;
  const ready = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('No inspectable WebKit target')), 10000);
    acceptTarget = id => { clearTimeout(timer); resolve(id); };
    socket.onerror = () => { clearTimeout(timer); reject(new Error('Inspector connection failed')); };
  });
  socket.onmessage = event => {
    let response = JSON.parse(event.data);
    if (response.method === 'Target.targetCreated') { acceptTarget(response.params.targetInfo.targetId); return; }
    if (response.method === 'Target.dispatchMessageFromTarget') response = JSON.parse(response.params.message);
    if (response.id > 1000000 && response.error) response.id -= 1000000;
    if (!response.id || !waiting.has(response.id)) return;
    const { resolve, reject, timer } = waiting.get(response.id);
    clearTimeout(timer);
    waiting.delete(response.id);
    response.error ? reject(new Error(response.error.message)) : resolve(response.result);
  };
  const targetId = await ready;
  const call = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => { waiting.delete(id); reject(new Error(`Inspector timeout: ${method}`)); }, 15000);
    waiting.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id: id + 1000000, method: 'Target.sendMessageToTarget', params: { targetId, message: JSON.stringify({ id, method, params }) } }));
  });
  return {
    call,
    async evaluate(expression) {
      let result = await call('Runtime.evaluate', { expression, returnByValue: false });
      if (result.wasThrown) throw new Error('Evaluation failed (details withheld to avoid secret output)');
      if (result.result.className === 'Promise') {
        result = await call('Runtime.awaitPromise', { promiseObjectId: result.result.objectId, returnByValue: true });
      } else if (result.result.objectId) {
        result = await call('Runtime.callFunctionOn', { objectId: result.result.objectId, functionDeclaration: 'function() { return this; }', returnByValue: true });
      }
      if (result.wasThrown) throw new Error('Evaluation failed (details withheld to avoid secret output)');
      return result.result.value;
    },
    async screenshot(path) {
      const metrics = await this.evaluate('({width: innerWidth, height: innerHeight})');
      const image = await call('Page.snapshotRect', { x: 0, y: 0, ...metrics, coordinateSystem: 'Viewport' });
      await writeFile(path, Buffer.from(image.dataURL.split(',')[1], 'base64'));
    },
    async close() {
      for (const { timer, reject } of waiting.values()) { clearTimeout(timer); reject(new Error('Inspector closed')); }
      waiting.clear();
      if (socket.readyState === WebSocket.CLOSED) return;
      // A target permits only one client: release it before the caller reconnects.
      const closed = once(socket, 'close', { signal: AbortSignal.timeout(5000) });
      socket.close();
      await closed;
    },
  };
}
