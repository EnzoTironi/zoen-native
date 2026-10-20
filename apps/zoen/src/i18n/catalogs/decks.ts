import { defineFeature } from '../catalog.ts';
export const decks = defineFeature(
  {
    shareVersion: 'Share presentation · version %{version}',
    exportFailed: 'The selected presentation version could not be shared. Try again.',
    opening: 'Opening your presentation…',
    saved: 'Saved in this chat',
    saving: 'Saving presentation…',
    pending: 'Unsaved changes',
    unavailable: 'This presentation could not be opened.',
    saveFailed: 'Your changes could not be saved. Keep the presentation open and try again.',
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
    opening: 'Abrindo sua apresentação…',
    saved: 'Salvo nesta conversa',
    saving: 'Salvando apresentação…',
    pending: 'Alterações pendentes',
    unavailable: 'Não foi possível abrir esta apresentação.',
    saveFailed:
      'Não foi possível salvar suas alterações. Mantenha a apresentação aberta e tente novamente.',
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
