import { defineFeature } from '../catalog.ts';
import { miniappsEn } from './miniapps.en.ts';
import { miniappsPtBR } from './miniapps.pt-BR.ts';

export const miniapps = defineFeature(miniappsEn, miniappsPtBR);
