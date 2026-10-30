import { defineFeature } from '../catalog.ts';

export const forwarding = defineFeature(
  {
    forwarded: 'Forwarded',
    forwardedFrom: 'Forwarded from %{name}',
    forward: 'Forward',
    forwardTo: 'Forward to…',
    startAnotherChatToForwardThisMessage: 'Start another chat to forward this message.',
    theMessageCouldNotBeForwarded: 'The message could not be forwarded.',
    reviewForwardAgain: 'The message changed. Choose a chat to review it again.',
    alreadyForwarded: 'Already forwarded',
    forwardAgain: 'Forward again',
    forwardRequiresAccount: 'Use your account to forward messages',
  },
  {
    forwarded: 'Encaminhada',
    forwardedFrom: 'Encaminhada de %{name}',
    forward: 'Encaminhar',
    forwardTo: 'Encaminhar para…',
    startAnotherChatToForwardThisMessage: 'Comece outra conversa para encaminhar esta mensagem.',
    theMessageCouldNotBeForwarded: 'Não foi possível encaminhar.',
    reviewForwardAgain: 'A mensagem mudou. Escolha uma conversa para revisá-la novamente.',
    alreadyForwarded: 'Já encaminhada',
    forwardAgain: 'Encaminhar novamente',
    forwardRequiresAccount: 'Use sua conta para encaminhar mensagens',
  },
);
