import { defineFeature } from '../catalog.ts';

export const replyOptions = defineFeature(
  {
    title: 'Reply options',
    select: 'Select quote',
    change: 'Change quote',
    apply: 'Apply',
    remove: 'Remove quote',
    cancel: 'Cancel reply',
    changed: 'This reply changed. Open its options again.',
  },
  {
    title: 'Opções de resposta',
    select: 'Selecionar citação',
    change: 'Alterar citação',
    apply: 'Aplicar',
    remove: 'Remover citação',
    cancel: 'Cancelar resposta',
    changed: 'Esta resposta mudou. Abra as opções novamente.',
  },
);
