// Copy for the host's language (pt-BR first; English fallback).
import { zoen } from '@zoen/ui';

const PT: Record<string, string> = {
  Close: 'Fechar',
  Back: 'Voltar',
  Services: 'Serviços',
  services: 'serviços',
  'Pick a day': 'Escolha o dia',
  'Pick a time': 'Escolha o horário',
  Book: 'Marcar',
  Confirm: 'Confirmar',
  'Cancel booking': 'Cancelar',
  'My bookings': 'Meus horários',
  Free: 'Livre',
  Taken: 'Ocupado',
  Pending: 'Aguardando',
  Confirmed: 'Confirmado',
  Cancelled: 'Cancelado',
  'No free slots': 'Sem horários livres neste dia',
  'Closed today': 'Fechado neste dia',
  'Ask sent': 'Pedido enviado',
  'Waiting for confirmation': 'Aguardando confirmação',
  Booked: 'Marcado',
  'Add to calendar': 'Pôr na agenda',
  'Added to your calendar': 'Na sua agenda',
  'Not added': 'Não adicionado',
  'Calendar isn’t available in this app host': 'Agenda não disponível neste app',
  'My availability': 'Minha disponibilidade',
  'Check my availability': 'Consultar minha disponibilidade',
  'Check this device for events marked busy. Only time ranges are used here; event names and details are not shared. Results stay private until you close this schedule.':
    'Consulte neste aparelho os compromissos marcados como ocupados. Só os intervalos de tempo são usados aqui; nomes e detalhes não são compartilhados. O resultado fica privado até você fechar esta agenda.',
  'Checking your calendar…': 'Consultando sua agenda…',
  'Calendar conflicts are marked below.': 'Os conflitos com sua agenda estão marcados abaixo.',
  'None of these business times conflict with events marked busy.':
    'Nenhum destes horários do negócio conflita com compromissos marcados como ocupados.',
  'Your calendar was not checked. No filter is applied.':
    'Não foi possível consultar sua agenda. Nenhum filtro foi aplicado.',
  'Try again': 'Tentar de novo',
  'Check again': 'Consultar de novo',
  'Cancel check': 'Cancelar consulta',
  'Remove calendar check': 'Remover consulta da agenda',
  'Busy in my calendar': 'Ocupado na minha agenda',
  'Business times are available, but all conflict with your calendar. Remove the calendar check to choose any business time.':
    'O negócio tem horários disponíveis, mas todos conflitam com sua agenda. Remova a consulta para escolher qualquer horário do negócio.',
  'Invalid calendar response': 'Resposta inválida da agenda',
  from: 'a partir de',
  min: 'min',
  Today: 'Hoje',
  Tomorrow: 'Amanhã',
  'Nothing booked yet': 'Nada marcado ainda',
  'Choose a service first': 'Escolha um serviço primeiro',
  'Send request': 'Pedir horário',
  'Request sent to the agent': 'Pedido enviado ao agente',
  'Cancel request sent': 'Pedido de cancelamento enviado',
  'Something went wrong': 'Algo deu errado',
  Price: 'Preço',
  Duration: 'Duração',
  Who: 'Quem',
  When: 'Quando',
  What: 'O quê',
  'Open hours': 'Horário de funcionamento',
  Requests: 'Pedidos',
  'No open requests': 'Nenhum pedido aberto',
  'Wants to cancel': 'Quer cancelar',
  Keep: 'Manter',
  'Outside the new hours': 'Fora do novo horário',
  'Someone booked this time just before you':
    'Alguém marcou esse horário um instante antes de você',
  'Pick another time.': 'Escolha outro horário.',
  Decline: 'Recusar',
  'Cancel requested': 'Cancelamento pedido',
  'Request cancelled': 'Pedido cancelado',
  'The agent is setting up this schedule': 'O agente está montando esta agenda',
  'You can add it to your calendar once it’s confirmed':
    'Quando for confirmado, dá para pôr na sua agenda',
};

// PT-BR is the default; English only when the host says so.
export const isPt = () =>
  !zoen.locale.toLowerCase().startsWith('en') ||
  zoen.capabilities.host === 'none' ||
  zoen.capabilities.host === 'unknown';
export const L = (s: string) => (isPt() ? (PT[s] ?? s) : s);
