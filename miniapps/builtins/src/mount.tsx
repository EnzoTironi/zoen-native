import { createRoot } from 'react-dom/client';
import type { ComponentType } from 'react';
import { zoen } from '@zoen/ui';
export function mountBuiltin(Component: ComponentType) {
  const root = document.getElementById('root');
  if (!root) throw new Error('Missing miniapp root');
  void zoen.ready();
  createRoot(root).render(<Component />);
}
