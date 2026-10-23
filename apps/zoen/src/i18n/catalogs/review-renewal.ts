import { defineFeature } from '../catalog.ts';

export const reviewRenewal = defineFeature(
  {
    expired: 'The approval period ended. This approval cannot start the action.',
    renew: 'Request a new review',
    confirmationTitle: 'Request another review?',
    confirmation:
      'This does not approve or repeat the action. The agent must check the exact original action and its result. If it can safely prepare a new request, review it and approve it separately.',
    requested:
      'New review requested. Wait for a new request from the agent; the previous approval remains expired.',
    superseded: 'A new request is ready. Its approval is separate from the previous one.',
    open: 'Open the new request',
    reconcile: 'The result needs reconciliation. Do not repeat an action whose result is unknown.',
    unavailable:
      'This action cannot be renewed here. Check its result and ask the agent for a new proposal if needed.',
    failed: 'The current review could not be loaded. The action will not be repeated.',
    unknown: 'The request is not confirmed. Refresh to check its state; it will not be resent.',
    refresh: 'Refresh review status',
    waiting: 'Checking review status…',
  },
  {
    expired: 'O prazo da aprovação terminou. Esta aprovação não pode iniciar a ação.',
    renew: 'Solicitar nova revisão',
    confirmationTitle: 'Solicitar outra revisão?',
    confirmation:
      'Isso não aprova nem repete a ação. O agente precisa conferir a ação original exata e seu resultado. Se puder preparar um novo pedido com segurança, revise e aprove esse pedido separadamente.',
    requested:
      'Nova revisão solicitada. Aguarde um novo pedido do agente; a aprovação anterior continua expirada.',
    superseded: 'Um novo pedido está pronto. Sua aprovação é separada da anterior.',
    open: 'Abrir o novo pedido',
    reconcile:
      'O resultado precisa ser reconciliado. Não repita uma ação com resultado desconhecido.',
    unavailable:
      'Não é possível renovar esta ação aqui. Confira o resultado e peça uma nova proposta ao agente, se necessário.',
    failed: 'Não foi possível carregar a revisão atual. A ação não será repetida.',
    unknown:
      'A solicitação não foi confirmada. Atualize para conferir o estado; ela não será reenviada.',
    refresh: 'Atualizar estado da revisão',
    waiting: 'Conferindo estado da revisão…',
  },
);
