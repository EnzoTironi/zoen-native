import { defineFeature } from '../catalog.ts';

export const agentDefinition = defineFeature(
  {
    title: 'Behavior',
    description: 'Review what your agent does and the rules it follows.',
    persona: 'What it does',
    rules: 'Rules',
    rule: 'Rule %{number}',
    addRule: 'Add a rule',
    removeRule: 'Remove rule %{number}',
    noRules: 'No additional rules.',
    review: 'Review changes',
    reviewing: 'Reviewing changes…',
    before: 'Before',
    after: 'Your changes',
    current: 'Current behavior',
    apply: 'Apply these changes',
    applying: 'Applying changes…',
    edit: 'Keep editing',
    saved: 'Behavior updated',
    conflict: 'The behavior has changed',
    conflictDetail:
      'Another change or access update was found. Review the current behavior before starting another edit.',
    unknown: 'Waiting for confirmation',
    unknownDetail:
      'Your change may still arrive. Check the result before starting another edit; it will not be sent again.',
    check: 'Check result',
    checking: 'Checking result…',
    another: 'Start another edit',
    anotherDetail:
      'This stops tracking this change. A change already sent may still arrive. It does not cancel the change or revoke the agent.',
    done: 'Done',
    revoked: 'This agent’s access has been revoked. Its behavior is read-only.',
    loading: 'Loading behavior…',
    unavailable: 'Open one of your agents in a signed-in account to review its behavior.',
    loadError:
      'The behavior could not be loaded. Update Zoen if needed, then try again. Editing stays locked until the current state is confirmed.',
    previewError:
      'These changes could not be reviewed. Check the text and rules, or load the latest behavior.',
    resultError:
      'The result could not be confirmed. The change may still arrive. Check again when connected.',
    dismissError:
      'The change being tracked has changed. Check the result before starting another edit.',
    retry: 'Try again',
    reload: 'Load latest behavior',
    discard: 'Discard unsent edits?',
    discardDetail: 'These local edits have not been applied.',
    discardConfirm: 'Discard edits',
    noPermissions: 'Behavior changes do not grant access to chats, tools or spending.',
  },
  {
    title: 'Comportamento',
    description: 'Confira o que seu agente faz e as regras que ele segue.',
    persona: 'O que ele faz',
    rules: 'Regras',
    rule: 'Regra %{number}',
    addRule: 'Adicionar regra',
    removeRule: 'Remover regra %{number}',
    noRules: 'Nenhuma regra adicional.',
    review: 'Revisar alterações',
    reviewing: 'Revisando alterações…',
    before: 'Antes',
    after: 'Suas alterações',
    current: 'Comportamento atual',
    apply: 'Aplicar estas alterações',
    applying: 'Aplicando alterações…',
    edit: 'Continuar editando',
    saved: 'Comportamento atualizado',
    conflict: 'O comportamento mudou',
    conflictDetail:
      'Foi encontrada outra alteração ou mudança de acesso. Confira o comportamento atual antes de iniciar outra edição.',
    unknown: 'Aguardando confirmação',
    unknownDetail:
      'Sua alteração ainda pode chegar. Consulte o resultado antes de iniciar outra edição; ela não será enviada de novo.',
    check: 'Consultar resultado',
    checking: 'Consultando resultado…',
    another: 'Iniciar outra edição',
    anotherDetail:
      'Isso encerra o acompanhamento desta alteração. Uma alteração já enviada ainda pode chegar. Ela não será cancelada e o agente não será revogado.',
    done: 'Concluir',
    revoked:
      'O acesso deste agente foi revogado. Seu comportamento está disponível apenas para leitura.',
    loading: 'Carregando comportamento…',
    unavailable: 'Abra um dos seus agentes em uma conta conectada para conferir o comportamento.',
    loadError:
      'Não foi possível carregar o comportamento. Atualize o Zoen se necessário e tente novamente. A edição fica bloqueada até confirmar o estado atual.',
    previewError:
      'Não foi possível revisar estas alterações. Confira o texto e as regras ou carregue o comportamento mais recente.',
    resultError:
      'Não foi possível confirmar o resultado. A alteração ainda pode chegar. Consulte novamente quando estiver conectado.',
    dismissError:
      'A alteração em acompanhamento mudou. Consulte o resultado antes de iniciar outra edição.',
    retry: 'Tentar novamente',
    reload: 'Carregar comportamento atual',
    discard: 'Descartar edições não enviadas?',
    discardDetail: 'Estas edições locais ainda não foram aplicadas.',
    discardConfirm: 'Descartar edições',
    noPermissions: 'Alterar o comportamento não concede acesso a conversas, ferramentas ou gastos.',
  },
);
