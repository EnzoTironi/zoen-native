import { defineFeature } from '../catalog.ts';
export const decks = defineFeature(
  {
    shareVersion: 'Share presentation · version %{version}',
    exportFailed: 'The selected presentation version could not be shared. Try again.',
    graphicsUnavailable:
      'This device could not start the presentation graphics. Try opening it again.',
    canvasUnavailable: 'The presentation canvas could not be started. Try opening it again.',
    runtimeUnavailable: 'The presentation editor could not start. Try opening it again.',
    resourceUnavailable: 'The presentation editor could not be downloaded. Try opening it again.',
    moduleUnavailable: 'The presentation editor could not be initialized. Try opening it again.',
    documentUnavailable: 'Your saved presentation could not be opened. Try opening it again.',
    opening: 'Opening your presentation…',
    saved: 'Saved in this chat',
    saving: 'Saving presentation…',
    pending: 'Unsaved changes',
    unavailable: 'This presentation could not be opened.',
    saveFailed: 'Your changes could not be saved. Keep the presentation open and try again.',
    editInProgress: 'Finish this edit before saving.',
    viewFailed: 'The presentation view could not update. Your presentation is still open.',
    editorStopped:
      'The presentation editor could not continue. Saved versions are still in this chat.',
    closeFailed:
      'The presentation is still open so you can keep your changes. Try saving again before leaving.',
    closeFirst: 'Save and close your presentation before leaving this screen.',
    accountChanged: 'The account or access to this presentation has changed. Open it again.',
    retrySave: 'Try saving again',
    discard: 'Discard unsaved changes',
    discardDetails:
      'Changes that have not been saved will be lost. Saved versions remain in this chat.',
  },
  {
    shareVersion: 'Compartilhar apresentação · versão %{version}',
    exportFailed:
      'Não foi possível compartilhar a versão selecionada da apresentação. Tente novamente.',
    graphicsUnavailable:
      'Este aparelho não conseguiu iniciar os gráficos da apresentação. Tente abrir novamente.',
    canvasUnavailable: 'Não foi possível iniciar a tela da apresentação. Tente abrir novamente.',
    runtimeUnavailable: 'O editor da apresentação não conseguiu iniciar. Tente abrir novamente.',
    resourceUnavailable: 'Não foi possível baixar o editor da apresentação. Tente abrir novamente.',
    moduleUnavailable:
      'Não foi possível inicializar o editor da apresentação. Tente abrir novamente.',
    documentUnavailable: 'Não foi possível abrir sua apresentação salva. Tente abrir novamente.',
    opening: 'Abrindo sua apresentação…',
    saved: 'Salvo nesta conversa',
    saving: 'Salvando apresentação…',
    pending: 'Alterações pendentes',
    unavailable: 'Não foi possível abrir esta apresentação.',
    saveFailed:
      'Não foi possível salvar suas alterações. Mantenha a apresentação aberta e tente novamente.',
    editInProgress: 'Conclua esta edição para salvar.',
    viewFailed: 'Não foi possível atualizar a visualização. Sua apresentação continua aberta.',
    editorStopped:
      'O editor da apresentação não pôde continuar. As versões salvas permanecem nesta conversa.',
    closeFailed:
      'A apresentação continua aberta para você manter suas alterações. Tente salvar novamente antes de sair.',
    closeFirst: 'Salve e feche sua apresentação antes de sair desta tela.',
    accountChanged: 'A conta ou o acesso a esta apresentação mudou. Abra-a novamente.',
    retrySave: 'Tentar salvar novamente',
    discard: 'Descartar alterações não salvas',
    discardDetails:
      'As alterações que não foram salvas serão perdidas. As versões salvas continuam nesta conversa.',
  },
);
