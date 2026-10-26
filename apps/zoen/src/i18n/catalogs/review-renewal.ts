import { defineFeature } from '../catalog.ts';

export const reviewRenewal = defineFeature(
  {
    expired:
      'This review period ended. Requesting a new review only asks the agent to prepare it; you must approve the new request separately.',
    historicalApproval:
      'Previous decision: approved once. This is not confirmation that the action ran.',
    renew: 'Request a new review',
    requested:
      'New review requested. Wait for a new request from the agent; the previous request remains expired.',
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
    expired:
      'O prazo desta revisão terminou. Solicitar nova revisão apenas pede ao agente que a prepare; você precisa aprovar o novo pedido separadamente.',
    historicalApproval:
      'Decisão anterior: aprovado uma vez. Isso não confirma que a ação foi executada.',
    renew: 'Solicitar nova revisão',
    requested:
      'Nova revisão solicitada. Aguarde um novo pedido do agente; o pedido anterior continua expirado.',
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
