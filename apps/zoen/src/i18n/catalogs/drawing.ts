import { defineFeature } from '../catalog.ts';

export const drawing = defineFeature(
  {
    saved: 'Saved in this chat',
    pending: 'Unsaved changes',
    saving: 'Saving drawing…',
    saveFailed: 'Your changes could not be saved. Keep the drawing open and try again.',
    retrySave: 'Try saving again',
    saveCopy: 'Save a copy in this chat',
    copying: 'Saving your copy…',
    copyFailed: 'The copy has not been confirmed yet. Try again to keep these exact changes.',
    saveNow: 'Save now',
    unavailable: 'This drawing could not be opened.',
    opening: 'Opening your drawing…',
    closeFailed:
      'The drawing is still open so you can keep your changes. Try saving again before leaving.',
    changed: 'This drawing has a newer version. Your edit is preserved in its history.',
    accountChanged: 'The account or access to this drawing has changed. Open it again.',
    closeFirst: 'Save and close your drawing before leaving this screen.',
    discard: 'Discard unsaved changes',
    discardDetails:
      'Changes that have not been saved will be lost. Saved versions remain in this chat.',
  },
  {
    saved: 'Salvo nesta conversa',
    pending: 'Alterações pendentes',
    saving: 'Salvando desenho…',
    saveFailed:
      'Não foi possível salvar suas alterações. Mantenha o desenho aberto e tente novamente.',
    retrySave: 'Tentar salvar novamente',
    saveCopy: 'Salvar uma cópia nesta conversa',
    copying: 'Salvando sua cópia…',
    copyFailed: 'A cópia ainda não foi confirmada. Tente novamente para manter estas alterações.',
    saveNow: 'Salvar agora',
    unavailable: 'Não foi possível abrir este desenho.',
    opening: 'Abrindo seu desenho…',
    closeFailed:
      'O desenho continua aberto para você manter suas alterações. Tente salvar novamente antes de sair.',
    changed: 'Este desenho tem uma versão mais recente. Sua edição foi preservada no histórico.',
    accountChanged: 'A conta ou o acesso a este desenho mudou. Abra-o novamente.',
    closeFirst: 'Salve e feche seu desenho antes de sair desta tela.',
    discard: 'Descartar alterações não salvas',
    discardDetails:
      'As alterações que não foram salvas serão perdidas. As versões salvas continuam nesta conversa.',
  },
);
