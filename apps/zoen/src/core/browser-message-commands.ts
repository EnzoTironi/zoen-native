import { array, integer, object, string } from '../../../../packages/zoen-ui/web/boundary';
import type { Messages } from '../../../../packages/zoen-ui/web/messages';
import {
  parseDevices,
  parseInvite,
  type Conversation,
} from '../../../../packages/zoen-ui/web/messages';
import { searchBrowser, searchBrowserMessages } from './browser-search';
import type { Persona } from './generated/Persona';
import type { Command, Snapshot, TimelineEntry } from './types';
type BrowserCommandContext = {
  entries(space: string): TimelineEntry[];
  conversations(): Conversation[];
  target(space: string, id: string | null): string;
  persona(identity: string, name: string): Persona;
  account(): Snapshot['account'];
  currentDevice(): string;
};

/** Borrow a linked transport for one dispatch; session/vault authority stays in BrowserDriver. */
export async function browserMessageCommand(
  command: Command,
  messages: Messages,
  context: BrowserCommandContext,
): Promise<{ result: unknown }> {
  let result: unknown = null;
  switch (command.op) {
    case 'timeline': {
      const entries = context.entries(command.space_id).filter((entry) => entry.in_thread === null);
      const end = command.before
        ? entries.findIndex((entry) => entry.id === command.before)
        : entries.length;
      if (end < 0) throw new Error('A conversa mudou. Abra-a novamente.');
      const start = Math.max(0, end - Math.min(200, Math.max(1, command.limit)));
      result = {
        entries: entries.slice(start, end),
        before: entries[start]?.id ?? null,
        has_more: start > 0,
      };
      break;
    }
    case 'send_message':
      await messages.post({
        space: command.space_id,
        text: command.text,
        replyTo: context.target(command.space_id, command.reply_to),
        thread: command.thread,
      });
      result = context.entries(command.space_id).at(-1);
      break;
    case 'start_direct':
    case 'create_group':
    case 'create_community':
    case 'add_member':
    case 'add_admin':
    case 'remove_member':
    case 'leave_space':
    case 'encrypt_chat':
      result = await messages.membershipRequest(command);
      break;
    case 'mark_read':
      await messages.markRead(command.space_id);
      break;
    case 'update_handle':
      await messages.updateHandle(command.handle);
      result = context.account();
      break;
    case 'link_code_check':
      result = await messages.linkRequest({ op: 'link_code_check', code: command.code });
      break;
    case 'link_device':
      result = await messages.linkDevice(command.code);
      break;
    case 'link_progress':
      result = await messages.linkProgress(command.device_id);
      break;
    case 'send_history':
      result = await messages.sendHistory(command.device_id);
      break;
    case 'load_older':
      result = await messages.loadOlder(command.space_id, Number(command.timeout_ms));
      break;
    case 'history_status':
      result = await messages.historyStatus();
      break;
    case 'receive_history':
      result = await messages.receiveHistory(Number(command.timeout_ms));
      break;
    case 'react':
      await messages.annotate(
        command.space_id,
        context.target(command.space_id, command.message_id),
        command.emoji ? { React: { emoji: command.emoji } } : 'Unreact',
      );
      break;
    case 'edit_message': {
      const { op: _operation, ...review } = command;
      await messages.editReviewed(review);
      break;
    }
    case 'delete_message':
      await messages.annotate(
        command.space_id,
        context.target(command.space_id, command.message_id),
        'Delete',
      );
      break;
    case 'thread': {
      const root = context
        .conversations()
        .find((chat) => chat.id === command.space_id)
        ?.messages.find(
          (message) => message.id === command.root_id || message.entry === command.root_id,
        );
      if (!root) throw new Error('A mensagem original não está disponível neste aparelho.');
      result = context
        .entries(command.space_id)
        .filter(
          (entry) =>
            entry.id === (root?.id ?? command.root_id) ||
            entry.in_thread === (root?.id ?? command.root_id),
        );
      break;
    }
    case 'message':
      result = context.entries(command.space_id).find((entry) => entry.id === command.message_id);
      if (!result) throw new Error('Mensagem indisponível nesta conversa.');
      break;
    case 'search_messages':
      result = searchBrowserMessages(
        context.conversations(),
        command.query,
        command.space_id,
        command.cursor,
        command.limit,
        (id, name) => context.persona(id, name),
      );
      break;
    case 'universal_search':
      result = searchBrowser(
        context.conversations(),
        command.query,
        command.kinds,
        command.limit,
        (id, name) => context.persona(id, name),
      );
      break;
    case 'forward_message': {
      const source = context
        .entries(command.source_space)
        .find((entry) => entry.id === command.message_id);
      if (!source || source.deleted || !('Message' in source.kind))
        throw new Error('Esta mensagem não pode ser encaminhada como texto.');
      await messages.post({
        space: command.target_space,
        text: source.kind.Message.text,
        forwarded: true,
      });
      break;
    }
    case 'devices':
      result = parseDevices(await messages.devices(), context.currentDevice());
      break;
    case 'unlink_device':
      await messages.unlinkDevice(command.device_id);
      break;
    case 'find_people': {
      const query = command.query.trim().replace(/^@/, '').toLocaleLowerCase();
      if (!query) {
        result = [];
        break;
      }
      const response = object(await messages.rpc('lookup', query));
      if (response.kind !== 'profiles') throw new Error('O servidor não retornou os perfis.');
      result = ((await messages.directoryProfiles(array(response.profiles))) as Persona[]).filter(
        (person) => !person.is_me,
      );
      break;
    }
    case 'agent_status': {
      const response = object(await messages.rpc('agent_status', command.agent_id));
      const status = string(response.status);
      if (response.kind !== 'agent_state' || !['active', 'paused', 'revoked'].includes(status))
        throw new Error('O servidor não retornou o estado do agente.');
      result = status === 'active' ? 'Active' : status === 'paused' ? 'Paused' : 'Revoked';
      break;
    }
    case 'control_agent':
    case 'revoke_agent_invite': {
      const response = object(
        await messages.rpc(
          command.op === 'control_agent' ? 'control_agent' : 'revoke_agent_invite',
          command.op === 'control_agent'
            ? JSON.stringify({ agent: command.agent_id, action: command.action.toLowerCase() })
            : command.agent_id,
        ),
      );
      if (response.kind !== 'done') throw new Error('O servidor não confirmou esta ação.');
      break;
    }
    case 'create_invite':
    case 'create_agent_invite': {
      const agent = command.op === 'create_agent_invite';
      const response = object(
        await messages.rpc(
          'create_invite',
          JSON.stringify({
            space: command.op === 'create_invite' ? command.space_id : '',
            agent: command.op === 'create_agent_invite' ? command.agent_id : null,
            role: 'Member',
            max_uses: agent ? 10_000 : 50,
            ttl_secs: (agent ? 365 : 7) * 24 * 3600,
          }),
        ),
      );
      if (response.kind !== 'invite') throw new Error('O servidor não criou o convite.');
      const code = string(response.code);
      const expires = string(response.expires_at_ms);
      if (!/^[0-9]+$/.test(expires) || BigInt(expires) > 9223372036854775807n)
        throw new Error('O convite tem uma validade inválida.');
      result = {
        code,
        link: agent ? `https://tryzoen.com/a/${code}` : `zoen://join/${code}`,
        expires_at_ms: expires,
      };
      break;
    }
    case 'preview_invite': {
      const response = object(await messages.rpc('preview_invite', command.code));
      if (response.kind !== 'preview') throw new Error('Este link não está mais disponível.');
      const records = [response.agent, response.inviter].filter((profile) => profile !== null);
      const people = (await messages.directoryProfiles(records)) as Persona[];
      const persona = (record: unknown) =>
        record ? (people.find((person) => person.id === object(record).id) ?? null) : null;
      result = {
        space_id: string(response.space),
        title: string(response.title),
        members: integer(response.members),
        kind: string(response.invite_kind),
        agent: persona(response.agent),
        inviter: persona(response.inviter),
      };
      break;
    }
    case 'join_invite': {
      const code = command.code.trim().toUpperCase();
      const response = object(await messages.rpc('preview_invite', code));
      if (response.invite_kind !== 'space')
        throw new Error(
          'Este link de agente ainda não pode ser aberto neste navegador. Abra no iPhone por enquanto.',
        );
      const invite = parseInvite(response);
      await messages.join({ invite, code });
      result = invite.space;
      break;
    }
    case 'item_discussion': {
      const discussion = object(await messages.documentRequest(command));
      if (discussion.space_id !== command.space_id || discussion.item_id !== command.item_id)
        throw new Error('A discussão pertence a outro arquivo ou conversa.');
      const rootId = discussion.root_id === null ? null : string(discussion.root_id);
      const root =
        rootId === null
          ? null
          : context.entries(command.space_id).find((entry) => entry.id === rootId);
      if (rootId !== null && (!root || root.in_thread !== null))
        throw new Error('A mensagem original não está disponível neste aparelho.');
      result = { item_id: command.item_id, space_id: command.space_id, root };
      break;
    }
    case 'new_mutation_id':
    case 'page_parse_markdown':
    case 'page_block_markdown':
    case 'items':
    case 'item':
    case 'item_at':
    case 'page':
    case 'page_at':
    case 'plan_page':
    case 'page_create':
    case 'page_import_markdown':
    case 'page_apply':
    case 'page_commit':
    case 'page_markdown':
    case 'page_export_version':
    case 'toggle_plan_line':
    case 'edit_plan_line':
    case 'add_plan_line':
    case 'remove_plan_line':
    case 'restore_plan_version':
    case 'restore_version':
    case 'undo':
    case 'app_document_read':
    case 'app_document_save':
    case 'app_document_copy':
      result = await messages.documentRequest(command);
      break;
    case 'agents':
    case 'agent_profile':
    case 'requests':
    case 'standing_decisions':
    case 'set_trust':
    case 'raise_budget':
    case 'decide_request':
    case 'revoke_standing':
    case 'preview_decisions':
    case 'agent_activity':
    case 'app_device_grants':
    case 'revoke_app_device':
      result = await messages.agentRequest(command);
      break;
    case 'mentions': {
      const references = (await messages.agentRequest(command)) as {
        space_id: string;
        space_title: string;
        message_id: string;
      }[];
      result = references.flatMap((reference) => {
        const message = context
          .conversations()
          .find((chat) => chat.id === reference.space_id)
          ?.messages.find(
            (message) =>
              message.id === reference.message_id || message.entry === reference.message_id,
          );
        const entry = message
          ? context.entries(reference.space_id).find((entry) => entry.id === message.id)
          : undefined;
        return entry
          ? [{ space_id: reference.space_id, space_title: reference.space_title, entry }]
          : [];
      });
      break;
    }
    case 'app_specs':
    case 'app_resource':
    case 'install_app':
    case 'app_call_tool':
      result = await messages.appRequest(command);
      break;
    case 'verify_log':
    case 'log_events_page':
      result = await messages.integrityRequest(command);
      break;
    case 'set_typing':
      await messages.typing(command.space_id, command.typing);
      break;
    default:
      throw new Error('Este recurso ainda está sendo integrado ao cliente web.');
  }
  return { result };
}
