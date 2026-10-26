export function memberRoleLabelKey(role: string | null) {
  return role === 'owner'
    ? 'community.owner'
    : role === 'admin'
      ? 'community.admin'
      : role === 'reader'
        ? 'community.reader'
        : 'community.member';
}
