// UI copy in the host's language (en base, pt-BR). Trail facts stay as written: they're
// proper names and park descriptions.
import { zoen } from '@zoen/ui';

const PT: Record<string, string> = {
  'All hikes': 'Todas as trilhas', 'Shady': 'Com sombra', 'Under 5 mi': 'Até 5 mi', 'Ocean views': 'Vista do mar',
  'options': 'opções', 'Compare': 'Comparar', 'Filter hikes': 'Filtrar trilhas', 'Lock in': 'Fechar', 'Going': 'Vamos',
  'mi away': 'mi daqui', 'Voted for': 'Voto em', 'Close': 'Fechar', 'Back': 'Voltar',
  'Distance': 'Distância', 'Elevation gain': 'Subida', 'Time': 'Tempo', 'Distance from you': 'Distância de você',
  'About': 'Uns', 'mi from you': 'mi de você', 'Show': 'Ver', 'About this trail': 'Sobre a trilha', 'Plan': 'Roteiro',
  'Album': 'Álbum', 'Add photos': 'Adicionar fotos', 'Share': 'Compartilhar', 'with group': 'com o grupo',
  'Add to calendar': 'Pôr na agenda', 'Vote': 'Votar', 'Voted': 'Votei', 'Compare hikes': 'Comparar trilhas',
  'Climb': 'Subida', 'Difficulty': 'Dificuldade', 'Route': 'Percurso', 'Terrain': 'Terreno', 'Shade': 'Sombra',
  'Best for': 'Melhor para', 'Your vote': 'Seu voto', 'Added to your calendar': 'Na sua agenda', 'Not added': 'Não adicionado',
  'Shared with the group': 'Compartilhado com o grupo', 'Open trail page': 'Abrir página da trilha',
  'Location isn’t available in this app host': 'Localização não disponível neste app',
  'Calendar isn’t available in this app host': 'Agenda não disponível neste app',
  'Photos aren’t available in this app host': 'Fotos não disponíveis neste app',
  'Moderate': 'Moderada', 'Easy': 'Fácil', 'Hard': 'Difícil', 'Out & back': 'Ida e volta', 'Loop': 'Circuito',
};

export const L = (s: string) => (zoen.locale.toLowerCase().startsWith('pt') ? PT[s] ?? s : s);
