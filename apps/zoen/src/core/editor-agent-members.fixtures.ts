import assert from 'node:assert/strict';
import { context } from './community-members.fixtures.ts';
import type { Snapshot } from './generated/Snapshot';
import type { SpaceMemberContext } from './generated/SpaceMemberContext';

export type AgentMemberMode =
  | 'ready'
  | 'missing'
  | 'none'
  | 'waiting'
  | 'pending'
  | 'reader'
  | 'foreign';
/** Explicit Core port facts in the generated 685 contract. Crypto/MLS remain Core tests. */
export function editorAgentMembers(snapshot: Snapshot, space: string) {
  let mode: AgentMemberMode = 'ready',
    registered = false,
    writable = true;
  const id = 'c'.repeat(64);
  const room = () => snapshot.conversations.find((c) => c.id === space)!;
  const manageable = () => writable && room().kind !== 'Direct';
  const profile = () => ({
    ...context().member!,
    id,
    kind: 'Agent' as const,
    name: 'My agent',
    handle: 'my_agent',
    owner_id: mode === 'foreign' ? 'foreign' : snapshot.account!.identity_id,
    is_mine: mode !== 'foreign',
  });
  const candidate = (): SpaceMemberContext => {
    const absent = mode === 'missing' || mode === 'none' || mode === 'foreign';
    const state = absent ? 'Removed' : mode === 'pending' ? 'Updating' : 'Active';
    return {
      ...context(),
      identity_id: snapshot.account!.identity_id,
      device_id: snapshot.account!.device_id,
      space_id: space,
      space_title: room().title,
      member_id: id,
      owned_agent: mode !== 'foreign',
      state,
      role: absent || mode === 'pending' ? null : mode === 'reader' ? 'reader' : 'member',
      member: absent ? null : profile(),
      candidate: absent || mode === 'pending' ? profile() : null,
      can_remove: false,
      can_add: absent && manageable(),
      role_options:
        absent && manageable()
          ? [
              { role: 'member', revision: 'c'.repeat(64) },
              { role: 'reader', revision: 'a'.repeat(64) },
              { role: 'admin', revision: 'b'.repeat(64) },
            ]
          : [],
      pending_role: mode === 'pending' ? 'member' : null,
      can_manage_members: manageable(),
      delivery_ready: mode === 'ready' || mode === 'reader',
      revision: String(
        ['ready', 'missing', 'none', 'waiting', 'pending', 'reader', 'foreign'].indexOf(mode) + 1,
      ).repeat(64),
    };
  };
  const preview = () => ({
    agent_id: id,
    offer_digest: 'd'.repeat(64),
    name: 'My agent',
    handle: 'my_agent',
    persona: 'Helps with documents.',
    rules: ['Ask before sharing.'],
    pairing_check: '123456',
    expires_ms: String(Date.now() + 300_000),
  });
  const receipt = () => ({
    receipt_id: 'enrollment-receipt',
    agent_id: id,
    offer_digest: 'd'.repeat(64),
    outcome: 'Sent',
  });
  const operations = [
    'find_member_candidates',
    'space_member_context',
    'set_member_role_reviewed',
    'agent_enrollment_status',
    'agent_enrollment_preview',
    'agent_enrollment_approve',
    'agent_enrollment_dismiss',
    'agent_status',
  ];
  return {
    id,
    set(next: AgentMemberMode) {
      mode = next;
    },
    writable(next: boolean) {
      writable = next;
    },
    current: candidate,
    handles: (op: string) => operations.includes(op),
    handle(command: any): unknown {
      if ('expected_identity_id' in command) {
        assert.equal(command.expected_identity_id, snapshot.account!.identity_id);
        assert.equal(command.expected_device_id, snapshot.account!.device_id);
      }
      switch (command.op) {
        case 'find_member_candidates': {
          assert.equal(command.owned_agents_only, true);
          if ((mode === 'none' && !registered) || mode === 'foreign') return [];
          if (command.query && !'my agent my_agent'.includes(String(command.query).toLowerCase()))
            return [];
          return [candidate()];
        }
        case 'space_member_context':
          if (room().kind === 'Direct') throw Error('Direct membership is not addable');
          if (command.identity_id === snapshot.account!.identity_id)
            return {
              ...context(),
              member_id: command.identity_id,
              identity_id: command.identity_id,
              device_id: snapshot.account!.device_id,
              space_id: space,
              can_manage_members: manageable(),
              can_remove: false,
              owned_agent: false,
              role: writable ? 'owner' : 'member',
              delivery_ready: true,
              member: { ...context().member!, id: command.identity_id },
            };
          return candidate();
        case 'set_member_role_reviewed':
          assert.equal(command.identity_id, id);
          assert.equal(
            command.observed_revision,
            candidate().role_options.find((option) => option.role === command.role)?.revision,
          );
          assert.equal(candidate().can_add, true);
          mode = 'pending';
          return candidate();
        case 'agent_enrollment_status':
          return registered ? { preview: preview(), delivery: receipt() } : null;
        case 'agent_enrollment_preview':
          return preview();
        case 'agent_enrollment_approve':
          registered = true;
          mode = 'missing';
          return receipt();
        case 'agent_enrollment_dismiss':
          return true;
        case 'agent_status':
          return registered ? 'Active' : null;
        default:
          throw Error(`Unexpected membership port ${command.op}`);
      }
    },
  };
}
