import { defineFeature } from '../catalog.ts';
import { mediaEn } from './media.en.ts';
import { mediaPtBR } from './media.pt-BR.ts';

export const media = defineFeature(mediaEn, mediaPtBR);
