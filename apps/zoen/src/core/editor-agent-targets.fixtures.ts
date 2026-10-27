import type { DialogOptions } from '../components/dialog-types';
import assert from 'node:assert/strict';
import type { AppEditTargets } from './generated/AppEditTargets';
import type { Persona } from './generated/Persona';
import type { Snapshot } from './generated/Snapshot';
import type { editorAgentMembers } from './editor-agent-members.fixtures';

/** A typed, explicit Core boundary. Cryptographic grants/revisions are tested in Core. */
export function editorAgentTargets(
  snapshot: Snapshot,
  space: string,
  members: ReturnType<typeof editorAgentMembers>,
  changed: () => void,
) {
  let lostAck = false;
  const published = new Map<string, unknown>();
  let multiple = false,
    permission = false,
    revision = 1;
  const confirmations: DialogOptions[] = [];
  let confirmGate: Promise<boolean> | null = null;
  const second = 'b'.repeat(64);
  const current = (version: number, sha: string, uri?: string): AppEditTargets => {
    const member = members.current(),
      writer = ['owner', 'admin', 'member'].includes(member.role ?? '');
    const agent = member.member ?? member.candidate;
    const personas = agent && writer && member.owned_agent ? [agent] : [];
    if (multiple && personas.length)
      personas.push({ ...personas[0]!, id: second, name: 'Second agent', handle: 'second_agent' });
    return {
      agents: personas.map((agent) => ({
        agent,
        delivery_ready: member.delivery_ready,
        permission_required: permission,
        eligible: member.delivery_ready && !permission,
      })),
      revision: (
        revision.toString(16) +
        member.revision.slice(0, 8) +
        version.toString(16) +
        sha.slice(0, 8) +
        (uri ? 'f' : 'e')
      ).padEnd(64, '0'),
    };
  };
  return {
    second,
    loseAck() {
      lostAck = true;
    },
    current,
    confirmations,
    multiple(value: boolean) {
      multiple = value;
      revision++;
    },
    permission(value: boolean) {
      permission = value;
      revision++;
    },
    holdConfirm(value: Promise<boolean> | null) {
      confirmGate = value;
    },
    confirm: async (options: DialogOptions) => {
      confirmations.push(options);
      return confirmGate ? await confirmGate : true;
    },
    handles: (op: string) => ['app_edit_targets', 'agent_profile', 'set_trust'].includes(op),
    handle(command: any, version: number, sha: string, uri?: string) {
      if (command.op === 'app_edit_targets') {
        assert.equal(command.input.expected_identity_id, snapshot.account!.identity_id);
        assert.equal(command.input.expected_device_id, snapshot.account!.device_id);
        assert.equal(command.input.space_id, space);
        assert.equal(command.input.expected_version, version);
        assert.equal(command.input.expected_sha256, sha);
        assert.equal(command.input.expected_resource_uri, uri);
        return current(version, sha, uri);
      }
      const agent = current(version, sha, uri).agents.find(
        (value) => value.agent.id === command.agent_id,
      )?.agent;
      assert.ok(agent);
      if (command.op === 'agent_profile')
        return {
          persona: agent,
          spaces: [{ space_id: space, space_title: 'Conversation', level: 'Act' }],
        };
      assert.equal(command.op, 'set_trust');
      assert.equal(command.space_id, space);
      assert.equal(command.level, 'Suggest');
      permission = false;
      revision++;
      changed();
      return true;
    },
    checkRequest(command: any, version: number, sha: string, uri?: string) {
      const prior = published.get(command.mutation_id);
      if (prior) {
        assert.deepEqual(command, prior);
        return;
      }
      const targets = current(version, sha, uri);
      assert.equal(command.expected_resource_uri, uri);
      assert.equal(command.expected_target_revision, targets.revision);
      assert.ok(
        targets.agents.some(
          (candidate) => candidate.eligible && candidate.agent.id === command.target_agent_id,
        ),
      );
      published.set(command.mutation_id, structuredClone(command));
      if (lostAck) {
        lostAck = false;
        throw Error('Committed request ACK lost');
      }
    },
    persona(id: string): Persona {
      const agent = members.current().member!;
      return { ...agent, id };
    },
  };
}
