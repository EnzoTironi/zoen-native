import { defineFeature } from '../catalog.ts';

export const quoteCollapse = defineFeature(
  {
    expandQuote: 'Expand quote',
    collapseQuote: 'Collapse quote',
    quoteExpanded: 'Expanded',
    quoteCollapsed: 'Collapsed',
  },
  {
    expandQuote: 'Expandir citação',
    collapseQuote: 'Recolher citação',
    quoteExpanded: 'Expandida',
    quoteCollapsed: 'Recolhida',
  },
);
