// Test-only X11 window enumeration, separate from input actions.
import { execFileSync } from 'node:child_process';

export function visibleWindows(name, run = execFileSync) {
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      return run('xdotool', ['search', '--onlyvisible', '--name', name], {
        encoding: 'utf8', timeout: 5000, stdio: ['ignore', 'pipe', 'pipe'],
      }).trim().split('\n').filter(Boolean);
    } catch (error) {
      const stderr = error.stderr?.toString();
      if (error.status === 1 && stderr === '' && error.stdout?.toString() === '') return [];
      // A disappearing WM frame invalidates XQueryTree. Discard partial output and repeat
      // only this read; never interpret an incomplete search as an absent consent dialog.
      const invalidated = error.status === 1 && stderr?.includes('BadWindow (invalid Window parameter)') && stderr.includes('(X_QueryTree)');
      if (!invalidated || attempt === 2) throw error;
    }
  }
}
