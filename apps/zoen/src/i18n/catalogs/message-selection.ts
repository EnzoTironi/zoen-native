import { defineFeature } from '../catalog.ts';

export const messageSelection = defineFeature(
  {
    selectMessages: 'Select',
    deleteOnThisDevice: 'Delete on this device',
    messageSelectionLimit: 'Select up to 100 messages. Deselect a message to choose another.',
    forwardSelectionProgress: '%{accepted} of %{count} forwarded',
    forwardSelectionReview: 'Review remaining messages',
    forwardSelectionSkipped: '%{count} skipped',
    notForwarded: 'Not forwarded',
    stopRemainingForwarding: 'Stop remaining',
    selectedMessages: { one: '%{count} message selected', other: '%{count} messages selected' },
    deleteSelectedMessages: { one: 'Delete %{count} message?', other: 'Delete %{count} messages?' },
    selectedMessagesRemovedForEveryone:
      'These messages will be removed for everyone in this conversation.',
  },
  {
    selectMessages: 'Selecionar',
    deleteOnThisDevice: 'Apagar deste aparelho',
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
    deleteSelectedMessages: {
      one: 'Apagar %{count} mensagem?',
      other: 'Apagar %{count} mensagens?',
    },
    selectedMessagesRemovedForEveryone:
      'Estas mensagens serão removidas para todos nesta conversa.',
  },
);
