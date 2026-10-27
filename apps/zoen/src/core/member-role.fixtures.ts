import { context, fixture } from './community-members.fixtures.ts';
import type { SpaceMemberContext } from './generated/SpaceMemberContext';
export { context, peer, scope, deferred, settle } from './community-members.fixtures.ts';
export const reader = { role: 'reader', revision: 'a'.repeat(64) };
export const admin = { role: 'admin', revision: 'b'.repeat(64) };
export function roleContext(patch: Partial<SpaceMemberContext> = {}): SpaceMemberContext {
  return { ...context(), role_options: [{ ...reader }, { ...admin }], ...patch };
}
export async function roleFixture() {
  const f = fixture();
  f.value(roleContext());
  await f.open();
  return {
    ...f,
    setRole(
      option = reader,
      confirm: (review: SpaceMemberContext) => Promise<boolean> = async () => true,
    ) {
      return f.session.setRole(option, confirm, f.session.ticket());
    },
  };
}
