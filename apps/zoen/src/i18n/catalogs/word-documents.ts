import { defineFeature } from '../catalog.ts';
export const wordDocuments = defineFeature(
  {
    shareVersion: 'Share document · version %{version}',
    exportFailed: 'The selected document version could not be shared. Try again.',
    graphicsUnavailable: 'This device could not start the document graphics. Try opening it again.',
    canvasUnavailable: 'The document canvas could not be started. Try opening it again.',
    runtimeUnavailable: 'The document editor could not start. Try opening it again.',
    resourceUnavailable: 'The document editor could not be downloaded. Try opening it again.',
    moduleUnavailable: 'The document editor could not be initialized. Try opening it again.',
    documentUnavailable: 'Your saved document could not be opened. Try opening it again.',
    opening: 'Opening your document…',
    saved: 'Saved in this chat',
    saving: 'Saving document…',
    pending: 'Unsaved changes',
    unavailable: 'This document could not be opened.',
    saveFailed: 'Your changes could not be saved. Keep the document open and try again.',
    editInProgress: 'Finish this edit before saving.',
    viewFailed: 'The document view could not update. Your document is still open.',
    editorStopped: 'The document editor could not continue. Saved versions are still in this chat.',
    closeFailed:
      'The document is still open so you can keep your changes. Try saving again before leaving.',
    closeFirst: 'Save and close your document before leaving this screen.',
    accountChanged: 'The account or access to this document has changed. Open it again.',
    retrySave: 'Try saving again',
    discard: 'Discard unsaved changes',
    discardDetails:
      'Changes that have not been saved will be lost. Saved versions remain in this chat.',
  },
  {
    shareVersion: 'Compartilhar documento · versão %{version}',
    exportFailed:
      'Não foi possível compartilhar a versão selecionada do documento. Tente novamente.',
    graphicsUnavailable:
      'Este aparelho não conseguiu iniciar os gráficos do documento. Tente abrir novamente.',
    canvasUnavailable: 'Não foi possível iniciar a tela do documento. Tente abrir novamente.',
    runtimeUnavailable: 'O editor do documento não conseguiu iniciar. Tente abrir novamente.',
    resourceUnavailable: 'Não foi possível baixar o editor do documento. Tente abrir novamente.',
    moduleUnavailable: 'Não foi possível inicializar o editor do documento. Tente abrir novamente.',
    documentUnavailable: 'Não foi possível abrir seu documento salvo. Tente abrir novamente.',
    opening: 'Abrindo seu documento…',
    saved: 'Salvo nesta conversa',
    saving: 'Salvando documento…',
    pending: 'Alterações pendentes',
    unavailable: 'Não foi possível abrir este documento.',
    saveFailed:
      'Não foi possível salvar suas alterações. Mantenha o documento aberto e tente novamente.',
    editInProgress: 'Conclua esta edição para salvar.',
    viewFailed: 'Não foi possível atualizar a visualização. Seu documento continua aberta.',
    editorStopped:
      'O editor do documento não pôde continuar. As versões salvas permanecem nesta conversa.',
    closeFailed:
      'O documento continua aberta para você manter suas alterações. Tente salvar novamente antes de sair.',
    closeFirst: 'Salve e feche seu documento antes de sair desta tela.',
    accountChanged: 'A conta ou o acesso a este documento mudou. Abra-o novamente.',
    retrySave: 'Tentar salvar novamente',
    discard: 'Descartar alterações não salvas',
    discardDetails:
      'As alterações que não foram salvas serão perdidas. As versões salvas continuam nesta conversa.',
  },
);
