import { defineFeature } from '../catalog.ts';

export const activity = defineFeature(
  {
    approvals: 'Approvals',
    mentions: 'Mentions',
    tasks: 'Tasks',
    resolved: 'Resolved',
    activity: 'Activity',
    reviewRequestsAndKeepUpWithYourChats: 'Review requests and keep up with your chats.',
    tryAgain: 'Try again',
    viewBudget: 'View budget',
    allCaughtUp: 'All caught up',
    requestsFromYourAgentsAppearHereWithThe:
      'Requests from your agents appear here with the context for each decision.',
    noDecisionsYet: 'No decisions yet',
    yourApprovalsAndRefusalsWillAppearHere: 'Your approvals and refusals will appear here.',
    noMentionsYet: 'No mentions yet',
    whenSomeoneMentionsYouInAChatThe:
      'When someone mentions you in a chat, the message will appear here.',
    noOpenTasks: 'No open tasks',
    tasksAndPlansWithUnfinishedLinesAppearHere:
      'Tasks and plans with unfinished lines appear here.',
    exploreMiniapps: 'Explore miniapps',
    agentNearLimit: '%{name} is approaching the limit',
    budgetUsage: '%{spent} of %{limit}',
  },
  {
    approvals: 'Aprovações',
    mentions: 'Menções',
    tasks: 'Tarefas',
    resolved: 'Decididos',
    activity: 'Atividade',
    reviewRequestsAndKeepUpWithYourChats: 'Revise os pedidos e acompanhe suas conversas.',
    tryAgain: 'Tentar novamente',
    viewBudget: 'Ver orçamento',
    allCaughtUp: 'Tudo em dia',
    requestsFromYourAgentsAppearHereWithThe:
      'Pedidos dos seus agentes aparecem aqui, com o contexto de cada decisão.',
    noDecisionsYet: 'Nenhuma decisão ainda',
    yourApprovalsAndRefusalsWillAppearHere: 'Suas aprovações e recusas aparecerão aqui.',
    noMentionsYet: 'Ninguém mencionou você',
    whenSomeoneMentionsYouInAChatThe:
      'Quando alguém chamar você numa conversa, a mensagem aparecerá aqui.',
    noOpenTasks: 'Nenhuma tarefa em aberto',
    tasksAndPlansWithUnfinishedLinesAppearHere:
      'Tarefas e planos com linhas pendentes aparecem aqui.',
    exploreMiniapps: 'Explorar miniapps',
    agentNearLimit: '%{name} está perto do limite',
    budgetUsage: '%{spent} de %{limit}',
  },
);
