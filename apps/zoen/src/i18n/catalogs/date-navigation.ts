import { defineFeature } from '../catalog.ts';
export const dateNavigation = defineFeature(
  {
    jumpToDate: 'Jump to Date',
    goToDate: 'Go to Date',
    dateNoMessages: 'No messages from this day are available on this device.',
    dateNavigationFailed: 'Could not open this day. Try again.',
  },
  {
    jumpToDate: 'Ir para data',
    goToDate: 'Ir para data',
    dateNoMessages: 'Não há mensagens desse dia disponíveis neste aparelho.',
    dateNavigationFailed: 'Não foi possível abrir esse dia. Tente novamente.',
  },
);
