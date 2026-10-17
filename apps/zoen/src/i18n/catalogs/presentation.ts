import { defineFeature } from '../catalog.ts';

export const presentation = defineFeature(
  {
    deletedMessage: 'Message deleted',
    chatAppearanceUpdated: 'Chat appearance updated',
    amountOutOfRange: 'This amount is outside the supported range.',
    enterBRLAmount: 'Enter an amount in BRL, such as 100.00.',
  },
  {
    deletedMessage: 'Mensagem apagada',
    chatAppearanceUpdated: 'Aparência da conversa atualizada',
    amountOutOfRange: 'Este valor está fora do intervalo permitido.',
    enterBRLAmount: 'Use um valor em reais, como 100,00.',
  },
);
