import { defineFeature } from '../catalog.ts';

export const imageReview = defineFeature(
  {
    low: 'Standard',
    medium: 'High quality',
    high: 'Highest quality',
    opaque: 'Solid background',
    transparent: 'Transparent background',
    unavailable: 'The image proposal could not be verified.',
  },
  {
    low: 'Padrão',
    medium: 'Alta qualidade',
    high: 'Máxima qualidade',
    opaque: 'Fundo sólido',
    transparent: 'Fundo transparente',
    unavailable: 'Não foi possível verificar a proposta da imagem.',
  },
);
