import { defineFeature } from '../catalog.ts';

export const messageSelection = defineFeature(
  {
    selectMessages: 'Select',
    messageSelectionLimit: 'Select up to 100 messages. Deselect a message to choose another.',
    forwardSelectionProgress: '%{accepted} of %{count} forwarded',
    forwardSelectionReview: 'Review remaining messages',
    forwardSelectionSkipped: '%{count} skipped',
    notForwarded: 'Not forwarded',
    stopRemainingForwarding: 'Stop remaining',
    selectedMessages: { one: '%{count} message selected', other: '%{count} messages selected' },
  },
  {
    selectMessages: 'Selecionar',
    messageSelectionLimit:
      'Selecione até 100 mensagens. Desmarque uma mensagem para escolher outra.',
    forwardSelectionProgress: '%{accepted} de %{count} encaminhadas',
    forwardSelectionReview: 'Revisar mensagens restantes',
    forwardSelectionSkipped: '%{count} ignoradas',
    notForwarded: 'Não encaminhada',
    stopRemainingForwarding: 'Encerrar restantes',
    selectedMessages: {
      one: '%{count} mensagem selecionada',
      other: '%{count} mensagens selecionadas',
    },
  },
);
