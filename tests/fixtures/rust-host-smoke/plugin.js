// Integration fixture: use the unmodified public Hermes Desktop plugin contract.
import { jsx, jsxs } from 'react/jsx-runtime';
import { useState } from 'react';
import { Button, host, useValue } from '@hermes/plugin-sdk';

function SmokePane() {
  const [count, setCount] = useState(0);
  const session = useValue(host.state.activeSessionId);
  return jsxs('section', {
    'data-testid': 'rust-host-plugin',
    style: { padding: '16px', color: 'var(--ui-text-secondary)' },
    children: [
      jsx('p', { children: 'Hermes SDK compatibility fixture' }),
      jsx('p', { 'data-testid': 'plugin-session', children: session ? 'Session connected' : 'No session selected' }),
      jsx(Button, {
        'data-testid': 'plugin-counter',
        onClick: () => setCount(value => value + 1),
        children: `Count ${count}`,
      }),
    ],
  });
}

export default {
  id: 'rust-host-smoke',
  name: 'Rust host compatibility smoke',
  register(ctx) {
    ctx.register({
      id: 'sdk-pane', area: 'panes', title: 'SDK smoke',
      data: { placement: 'bottom', dock: { pane: 'workspace', pos: 'bottom' }, height: '180px' },
      render: () => jsx(SmokePane, {}),
    });
  },
};
