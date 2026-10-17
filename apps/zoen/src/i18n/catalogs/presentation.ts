import { defineFeature } from '../catalog.ts';

export const presentation = defineFeature(
  {
    activityTyping: 'Typing…',
    activityPersonTyping: '%{name} is typing…',
    activityPairTyping: '%{first} and %{second} are typing…',
    activityPeopleTyping: {
      one: '%{count} person is typing…',
      other: '%{count} people are typing…',
    },
    activityProcessing: 'Processing…',
    activityPersonProcessing: '%{name} is processing…',
    activityBuilding: 'Creating an app…',
    activityPersonBuilding: '%{name} is creating an app…',
    activityCall: 'In a call',
    activityPersonCall: '%{name} is in a call',
    activityOnline: 'Online',
    deletedMessage: 'Message deleted',
    chatAppearanceUpdated: 'Chat appearance updated',
    amountOutOfRange: 'This amount is outside the supported range.',
    enterBRLAmount: 'Enter an amount in BRL, such as 100.00.',
  },
  {
    activityTyping: 'Digitando…',
    activityPersonTyping: '%{name} está digitando…',
    activityPairTyping: '%{first} e %{second} estão digitando…',
    activityPeopleTyping: {
      one: '%{count} pessoa está digitando…',
      other: '%{count} pessoas estão digitando…',
    },
    activityProcessing: 'Processando…',
    activityPersonProcessing: '%{name} está processando…',
    activityBuilding: 'Criando um app…',
    activityPersonBuilding: '%{name} está criando um app…',
    activityCall: 'Em uma chamada',
    activityPersonCall: '%{name} está em uma chamada',
    activityOnline: 'Online',
    deletedMessage: 'Mensagem apagada',
    chatAppearanceUpdated: 'Aparência da conversa atualizada',
    amountOutOfRange: 'Este valor está fora do intervalo permitido.',
    enterBRLAmount: 'Use um valor em reais, como 100,00.',
  },
);
