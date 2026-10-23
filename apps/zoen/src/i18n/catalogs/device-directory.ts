import { defineFeature } from '../catalog.ts';

export const deviceDirectory = defineFeature(
  {
    current: 'This device',
    others: 'Other devices',
    revoked: 'Revoked devices',
    fingerprint: 'Device fingerprint',
    details: 'Device details',
    noOtherDevices: 'No other devices',
    readFailed: 'Could not load your devices.',
    revokeFailed: 'Revocation could not be confirmed. Refresh to check its access.',
    refresh: 'Refresh devices',
    retry: 'Try again',
    close: 'Close',
    link: 'Link a device',
    revoke: 'Revoke access',
    revokeTitle: 'Revoke access for %{name}?',
    revokeDetail:
      'It will lose access to your account and encrypted chats. Files already downloaded may remain on it.',
  },
  {
    current: 'Este aparelho',
    others: 'Outros aparelhos',
    revoked: 'Aparelhos revogados',
    fingerprint: 'Identificação do aparelho',
    details: 'Detalhes do aparelho',
    noOtherDevices: 'Nenhum outro aparelho',
    readFailed: 'Não foi possível carregar seus aparelhos.',
    revokeFailed: 'Não foi possível confirmar a revogação. Atualize para verificar o acesso.',
    refresh: 'Atualizar aparelhos',
    retry: 'Tentar novamente',
    close: 'Fechar',
    link: 'Vincular um aparelho',
    revoke: 'Revogar acesso',
    revokeTitle: 'Revogar acesso de %{name}?',
    revokeDetail:
      'Ele perderá acesso à sua conta e às conversas criptografadas. Arquivos já baixados podem permanecer nele.',
  },
);
