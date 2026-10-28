import { defineFeature } from '../catalog.ts';
export const messageInfo = defineFeature(
  {
    title: 'Message info',
    sent: 'Sent',
    edited: 'Edited',
    read: 'Read',
    delivered: 'Delivered',
    pending: 'Awaiting confirmation',
    timeUnavailable: 'Time not provided',
    authorClock: 'Times reported by the sender’s device.',
    peerClock: 'Read times are reported by participants’ devices, not this device’s receipt time.',
    readReceipts: 'Read receipts',
    smallConversationOnly: 'Participant details are available in direct chats and small groups.',
    unavailable: 'Message info is no longer available.',
    loadFailed: 'Could not load message info.',
  },
  {
    title: 'Informações da mensagem',
    sent: 'Enviada',
    edited: 'Editada',
    read: 'Lida',
    delivered: 'Entregue',
    pending: 'Aguardando confirmação',
    timeUnavailable: 'Horário não informado',
    authorClock: 'Horários informados pelo dispositivo de quem enviou.',
    peerClock:
      'Os horários de leitura são informados pelos dispositivos dos participantes, não pelo recebimento neste aparelho.',
    readReceipts: 'Confirmações de leitura',
    smallConversationOnly:
      'Os detalhes dos participantes estão disponíveis em conversas diretas e grupos pequenos.',
    unavailable: 'As informações da mensagem não estão mais disponíveis.',
    loadFailed: 'Não foi possível carregar as informações da mensagem.',
  },
);
