#!/usr/bin/env node
// Build upstream + our reviewable UI fixes without editing the pinned submodule.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, symlinkSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { test: { type: 'boolean', default: false } } });

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const upstream = join(root, 'upstream/hermes-agent');
const build = join(root, '.build');
const git = (cwd, ...args) => execFileSync('git', ['-C', cwd, ...args], {
  encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'],
}).trim();

// The gitlink, not a second version registry, is the source of truth.
const entry = git(root, 'ls-files', '--stage', '--', 'upstream/hermes-agent');
const revision = /^160000 ([a-f0-9]{40}) 0\tupstream\/hermes-agent$/.exec(entry)?.[1];
if (!revision || git(upstream, 'rev-parse', 'HEAD') !== revision) {
  throw new Error('Initialize the pinned submodule with git submodule update --init --recursive.');
}
mkdirSync(build, { recursive: true });
const source = mkdtempSync(join(build, 'renderer-source-'));
try {
  // A disposable checkout keeps provenance intact and prevents reset/clean of user work.
  git(root, 'clone', '--quiet', '--shared', '--no-checkout', upstream, source);
  git(source, 'checkout', '--quiet', '--detach', revision);
  for (const patch of ['desktop-accessibility.patch', 'desktop-downloads.patch']) {
    git(source, 'apply', '--check', join(root, 'patches', patch));
    git(source, 'apply', join(root, 'patches', patch));
  }

  const { prepareWebappWorkspace } = await import(pathToFileURL(
    join(upstream, 'apps/desktop/scripts/build-webapp.mjs'),
  ).href);
  const { buildWebapp } = await import(pathToFileURL(
    join(upstream, 'scripts/build/webapp.mjs'),
  ).href);
  // Reuse upstream's locked, script-free dependency preparation, receipt, and atomic publisher.
  const prepared = prepareWebappWorkspace({
    source, workspace: join(upstream, '.build/webapp-workspace'),
  });
  if (values.test) {
    // Some UI tests read Python-side fixtures; run in the full checkout, not the
    // frontend-only build mirror. Dependencies still come from the locked workspace.
    for (const directory of ['node_modules', 'apps/desktop/node_modules']) {
      const dependencies = join(prepared.workspace, directory);
      if (existsSync(dependencies)) {
        symlinkSync(dependencies, join(source, directory), process.platform === 'win32' ? 'junction' : 'dir');
      }
    }
    execFileSync(process.execPath, [join(prepared.workspace, 'node_modules/vitest/vitest.mjs'),
      'run', '--project', 'ui', 'src/components/pane-shell', 'src/components/ui',
      'src/app/starmap', 'src/app/right-sidebar', 'src/lib/session-export.test.ts', 'src/lib/download.test.ts'], {
      cwd: join(source, 'apps/desktop'), stdio: 'inherit',
    });
  }
  const result = await buildWebapp({ ...prepared, out: join(build, 'webapp-dist') });
  console.log(`Built patched Hermes ${revision} at ${result.out}`);
} finally {
  // Only this invocation's mkdtemp checkout; never the submodule or previous good product.
  rmSync(source, { recursive: true, force: true });
}
