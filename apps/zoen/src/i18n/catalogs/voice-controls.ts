import { defineFeature } from '../catalog.ts';
export const voiceControls = defineFeature(
  {
    holdHint:
      'Hold to record, slide up to lock, or slide left to cancel. Tap to record hands-free.',
    slideToCancel: 'Slide to cancel',
    releaseToReview: 'Release to review',
    locked: 'Recording locked',
    pause: 'Pause recording',
    resume: 'Resume recording',
    paused: 'Paused',
    finish: 'Finish recording',
  },
  {
    holdHint:
      'Segure para gravar, deslize para cima para travar ou para a esquerda para cancelar. Toque para gravar sem segurar.',
    slideToCancel: 'Deslize para cancelar',
    releaseToReview: 'Solte para revisar',
    locked: 'Gravação travada',
    pause: 'Pausar gravação',
    resume: 'Continuar gravação',
    paused: 'Pausado',
    finish: 'Concluir gravação',
  },
);
