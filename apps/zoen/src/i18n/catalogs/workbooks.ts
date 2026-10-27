import { defineFeature } from '../catalog.ts';
export const workbooks = defineFeature(
  {
    shareVersion: 'Share spreadsheet · version %{version}',
    exportFailed: 'The selected spreadsheet version could not be shared. Try again.',
    graphicsUnavailable:
      'This device could not start the spreadsheet graphics. Try opening it again.',
    canvasUnavailable: 'The spreadsheet canvas could not be started. Try opening it again.',
    runtimeUnavailable: 'The spreadsheet editor could not start. Try opening it again.',
    resourceUnavailable: 'The spreadsheet editor could not be downloaded. Try opening it again.',
    moduleUnavailable: 'The spreadsheet editor could not be initialized. Try opening it again.',
    documentUnavailable: 'Your saved spreadsheet could not be opened. Try opening it again.',
    opening: 'Opening your spreadsheet…',
    saved: 'Saved in this chat',
    saving: 'Saving spreadsheet…',
    pending: 'Unsaved changes',
    unavailable: 'This spreadsheet could not be opened.',
    saveFailed: 'Your changes could not be saved. Keep the spreadsheet open and try again.',
    editInProgress: 'Finish this edit before saving.',
    viewFailed: 'The spreadsheet view could not update. Your spreadsheet is still open.',
    editorStopped:
      'The spreadsheet editor could not continue. Saved versions are still in this chat.',
    closeFailed:
      'The spreadsheet is still open so you can keep your changes. Try saving again before leaving.',
    closeFirst: 'Save and close your spreadsheet before leaving this screen.',
    accountChanged: 'The account or access to this spreadsheet has changed. Open it again.',
    retrySave: 'Try saving again',
    discard: 'Discard unsaved changes',
    discardDetails:
      'Changes that have not been saved will be lost. Saved versions remain in this chat.',
  },
  {
    shareVersion: 'Compartilhar planilha · versão %{version}',
    exportFailed:
      'Não foi possível compartilhar a versão selecionada da planilha. Tente novamente.',
    graphicsUnavailable:
      'Este aparelho não conseguiu iniciar os gráficos da planilha. Tente abrir novamente.',
    canvasUnavailable: 'Não foi possível iniciar a tela da planilha. Tente abrir novamente.',
    runtimeUnavailable: 'O editor da planilha não conseguiu iniciar. Tente abrir novamente.',
    resourceUnavailable: 'Não foi possível baixar o editor da planilha. Tente abrir novamente.',
    moduleUnavailable: 'Não foi possível inicializar o editor da planilha. Tente abrir novamente.',
    documentUnavailable: 'Não foi possível abrir sua planilha salva. Tente abrir novamente.',
    opening: 'Abrindo sua planilha…',
    saved: 'Salvo nesta conversa',
    saving: 'Salvando planilha…',
    pending: 'Alterações pendentes',
    unavailable: 'Não foi possível abrir esta planilha.',
    saveFailed:
      'Não foi possível salvar suas alterações. Mantenha a planilha aberta e tente novamente.',
    editInProgress: 'Conclua esta edição para salvar.',
    viewFailed: 'Não foi possível atualizar a visualização. Sua planilha continua aberta.',
    editorStopped:
      'O editor da planilha não pôde continuar. As versões salvas permanecem nesta conversa.',
    closeFailed:
      'A planilha continua aberta para você manter suas alterações. Tente salvar novamente antes de sair.',
    closeFirst: 'Salve e feche sua planilha antes de sair desta tela.',
    accountChanged: 'A conta ou o acesso a esta planilha mudou. Abra-a novamente.',
    retrySave: 'Tentar salvar novamente',
    discard: 'Descartar alterações não salvas',
    discardDetails:
      'As alterações que não foram salvas serão perdidas. As versões salvas continuam nesta conversa.',
  },
);
