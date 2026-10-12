import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { DrawerFocusFixture } from './focus-fixture';
import '@zoen/ui/shell.css';
import './focus-fixture.css';

const root = document.getElementById('focus-root');
if (!root) throw new Error('The drawer focus fixture needs a root element.');
createRoot(root).render(
  <StrictMode>
    <DrawerFocusFixture />
  </StrictMode>,
);
