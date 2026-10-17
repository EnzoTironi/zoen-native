import { defineFeature } from '../catalog.ts';

export const auth = defineFeature(
  {
    continueFailed: 'Could not continue. Please try again.',
    prepareFailed: 'Could not prepare sign-in.',
    appleUnavailable: 'Apple is unavailable in this version.',
    googleUnavailable: 'Google is unavailable in this version.',
    continueGoogle: 'Continue with Google',
    continueApple: 'Continue with Apple',
    preparingAccount: 'Preparing your Zoen…',
    preparingSignIn: 'Preparing sign-in…',
  },
  {
    continueFailed: 'Não foi possível continuar. Tente novamente.',
    prepareFailed: 'Não foi possível preparar a entrada.',
    appleUnavailable: 'Apple indisponível nesta versão.',
    googleUnavailable: 'Google indisponível nesta versão.',
    continueGoogle: 'Continuar com Google',
    continueApple: 'Continuar com Apple',
    preparingAccount: 'Preparando seu Zoen…',
    preparingSignIn: 'Preparando entrada…',
  },
);
