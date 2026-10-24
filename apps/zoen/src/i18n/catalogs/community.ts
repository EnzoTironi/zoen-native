import { defineFeature } from '../catalog.ts';

export const community = defineFeature(
  {
    member: 'Member',
    owner: 'Owner',
    admin: 'Administrator',
    actions: 'Member actions',
    actionsHint: 'Touch and hold for member actions.',
    profile: 'View profile',
    remove: 'Remove member',
    removeNamed: 'Remove %{name}?',
    removeFrom: 'Remove this person from “%{title}”?',
    removing: 'Removal pending',
    updating: 'Membership updating',
    readFailed: 'Could not verify this member. Try again.',
    actionFailed:
      'Removal could not be confirmed. Check the current membership before trying again.',
  },
  {
    member: 'Participante',
    owner: 'Proprietário',
    admin: 'Administrador',
    actions: 'Ações do participante',
    actionsHint: 'Toque e segure para ver as ações do participante.',
    profile: 'Ver perfil',
    remove: 'Remover participante',
    removeNamed: 'Remover %{name}?',
    removeFrom: 'Remover esta pessoa de “%{title}”?',
    removing: 'Remoção pendente',
    updating: 'Atualizando participantes',
    readFailed: 'Não foi possível verificar este participante. Tente novamente.',
    actionFailed:
      'Não foi possível confirmar a remoção. Confira os participantes antes de tentar novamente.',
  },
);
