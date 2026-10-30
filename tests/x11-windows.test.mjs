// A failed X11 tree traversal is not evidence that a window or consent dialog is absent.
import test from 'node:test';
import assert from 'node:assert/strict';
import { visibleWindows } from '../scripts/x11-windows.mjs';

const staleTree = () => Object.assign(new Error('fixture X11 tree invalidated'), {
  status: 1,
  stdout: '999\n',
  stderr: 'X Error of failed request: BadWindow (invalid Window parameter)\nMajor opcode of failed request: 15 (X_QueryTree)\n',
});

test('window discovery discards an invalidated snapshot and repeats only the read', () => {
  let calls = 0;
  const result = visibleWindows('fixture', (command, args) => {
    assert.equal(command, 'xdotool');
    assert.deepEqual(args, ['search', '--onlyvisible', '--name', 'fixture']);
    if (calls++ === 0) throw staleTree();
    return '11\n22\n';
  });
  assert.deepEqual(result, ['11', '22']);
  assert.equal(calls, 2);
});

test('only a completed no-match search proves absence; other failures stay fatal', () => {
  const noMatches = Object.assign(new Error('fixture no match'), { status: 1, stdout: '', stderr: '' });
  assert.deepEqual(visibleWindows('fixture', () => { throw noMatches; }), []);
  const unrelated = Object.assign(new Error('fixture display failure'), { status: 1, stderr: 'Unable to open display' });
  assert.throws(() => visibleWindows('fixture', () => { throw unrelated; }), error => error === unrelated);
  let calls = 0;
  assert.throws(() => visibleWindows('fixture', () => { calls++; throw staleTree(); }), /tree invalidated/);
  assert.equal(calls, 3, 'persistent invalidation must fail, never report absence');
});
