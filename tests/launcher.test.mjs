import test from 'node:test';
import assert from 'node:assert/strict';
import { createActions } from '../ui/actions.mjs';

test('connecting reports immediate progress, single-flights clicks, and recovers after failure', async () => {
  const frames = [];
  let fail;
  let calls = 0;
  const actions = createActions(() => { calls++; return new Promise((_, reject) => { fail = reject; }); }, value => frames.push(value));
  const pending = actions.connect({ profile: 'work' });
  assert.equal(frames.at(-1)?.busy, true);
  await actions.connect({ profile: 'work' });
  assert.equal(calls, 1, 'double-click must not launch parallel gateways');
  fail({ code: 'webapp_disabled', message: 'The Webapp is disabled.', recovery: 'Enable it explicitly in Hermes.' });
  await pending;
  assert.equal(frames.at(-1).busy, false);
  assert.equal(frames.at(-1).error.message, 'The Webapp is disabled.');
  assert.match(frames.at(-1).error.recovery, /explicitly/);
});

test('a completed build does not claim that a workspace was opened', async () => {
  const frames = [];
  const calls = [];
  const actions = createActions(async (command, args) => calls.push([command, args]), frame => frames.push(frame));
  await actions.build({ mode: 'local' });
  assert.equal(calls[0][0], 'build_renderer');
  assert.match(frames.at(-1).message, /built/i);
  assert.doesNotMatch(frames.at(-1).message, /opened/i);
  await actions.connect({ mode: 'local' });
  assert.equal(calls[1][0], 'launch_workspace');
});
