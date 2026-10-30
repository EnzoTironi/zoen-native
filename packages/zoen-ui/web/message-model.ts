import { parseCursorRepairStatus } from './cursor-reconciliation.ts';
// Pure boundary parsing; no session, transport or persistence owner.
import { string, object, integer, boolean, array, publicKey, bytes } from './boundary.ts';
import { type FileDoc, parseMessageMedia, parseFile } from './message-file-model.ts';
import { type ConnectionState } from './transport.ts';

export function nullableText(value: unknown): string | null {
  return value === null ? null : string(value);
}
export function timestamp(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value))
    throw new Error('Invalid message timestamp.');
  return value;
}
export function parseItemEdit(input: unknown, space: string) {
  if (input === null || input === undefined) return null;
  const value = object(input);
  const space_id = string(value.space_id);
  const item_id = string(value.item_id);
  const version = integer(value.version);
  if (space_id !== space || !item_id || version < 2 || version > 4294967295)
    throw new Error('Invalid mini-app activity identity or version.');
  return {
    space_id,
    item_id,
    title: string(value.title),
    version,
    note: string(value.note),
    is_undo: boolean(value.is_undo),
  };
}
export function parseMessage(input: unknown, space: string) {
  const value = object(input);
  const delivery = string(value.delivery);
  if (delivery !== 'confirmed' && delivery !== 'sending' && delivery !== 'failed')
    throw new Error('Invalid message delivery state.');
  const ticks = value.ticks === null || value.ticks === undefined ? null : string(value.ticks);
  if (ticks !== null && ticks !== 'sent' && ticks !== 'delivered' && ticks !== 'read')
    throw new Error('Invalid message receipt.');
  const quote = value.quote ? object(value.quote) : null;
  const sequence = value.seq === null || value.seq === undefined ? null : string(value.seq);
  if (sequence !== null && (!/^[0-9]+$/.test(sequence) || BigInt(sequence) > 18446744073709551615n))
    throw new Error('Invalid message sequence.');
  let text = string(value.text);
  const item_edit = parseItemEdit(value.item_edit, space);
  if (
    item_edit &&
    (text !== item_edit.note ||
      value.card ||
      value.file ||
      value.quote ||
      value.thread ||
      value.deleted === true ||
      value.edited === true ||
      value.forwarded === true ||
      value.media ||
      array(value.reactions ?? []).length)
  )
    throw new Error('Invalid mixed mini-app activity.');
  let file: FileDoc | null = null;
  let fileItemId: string | null = null;
  let fileVersion: number | null = null;
  let media: ReturnType<typeof parseMessageMedia> = null;
  // A bad attachment must not prevent saving or restoring the conversation's sync state.
  if (value.file) {
    try {
      file = parseFile(value.file);
      fileItemId = string(value.file_item_id);
      fileVersion = integer(value.file_version);
      if (!fileItemId || fileVersion < 1) throw new Error('Invalid file identity.');
      media = parseMessageMedia(value.media);
      if (media && (file.voice || (value.forwarded === true && media.group)))
        throw new Error('Invalid attachment context.');
    } catch {
      file = null;
      fileItemId = null;
      fileVersion = null;
      media = null;
      text = 'Anexo indisponível';
    }
  }
  return {
    id: string(value.id),
    entry: value.entry ? string(value.entry) : null,
    text_revision: value.text_revision == null ? null : string(value.text_revision),
    seq: sequence,
    author: publicKey(value.author),
    name: string(value.name),
    text,
    at_ms: timestamp(value.at_ms),
    delivery,
    failure: nullableText(value.failure),
    ticks: ticks as 'sent' | 'delivered' | 'read' | null,
    edited: value.edited === true,
    deleted: value.deleted === true,
    forwarded: value.forwarded === true,
    reactions: array(value.reactions ?? []).map((r) => {
      const x = object(r);
      return {
        emoji: string(x.emoji),
        count: integer(x.count),
        mine: x.mine === true,
        participants: array(x.participants ?? []).map((p) => {
          const participant = object(p);
          return { identity: publicKey(participant.identity), name: string(participant.name) };
        }),
      };
    }),
    quote: quote
      ? { id: string(quote.id), name: string(quote.name), text: string(quote.text) }
      : null,
    thread: value.thread ? string(value.thread) : null,
    replies: integer(value.replies ?? 0),
    repliers: array(value.repliers ?? []).map(string),
    last_reply_ms: typeof value.last_reply_ms === 'number' ? value.last_reply_ms : null,
    file,
    media: value.deleted === true ? null : media,
    file_item_id: fileItemId,
    file_version: fileVersion,
    card: value.card ? parseCard(value.card) : null,
    item_edit,
  };
}

export function parseCard(input: unknown) {
  const value = object(input);
  const app = value.app === null ? null : object(value.app);
  const trust = app ? string(app.trust) : null;
  if (trust && !['Listen', 'Suggest', 'Act', 'Autonomous'].includes(trust))
    throw new Error('Invalid mini-app trust.');
  return {
    item_id: string(value.item_id),
    title: string(value.title),
    summary: string(value.summary),
    kind_label: string(value.kind_label),
    kind_id: string(value.kind_id),
    version: integer(value.version),
    total_cents:
      value.total_cents === null ? null : decimal(value.total_cents, 9223372036854775807n, true),
    budget_cents:
      value.budget_cents === null ? null : decimal(value.budget_cents, 9223372036854775807n, true),
    line_count: integer(value.line_count),
    done_count: integer(value.done_count),
    app: app
      ? {
          app_id: string(app.app_id),
          resource_uri: string(app.resource_uri),
          name: string(app.name),
          view_json: string(app.view_json),
          headline: string(app.headline),
          metrics: array(app.metrics).map((metric) => {
            const parsed = object(metric);
            if (typeof parsed.value !== 'number' || !Number.isFinite(parsed.value))
              throw new Error('Invalid mini-app metric.');
            return { label: string(parsed.label), value: parsed.value };
          }),
          last_action: nullableText(app.last_action),
          trust: trust as 'Listen' | 'Suggest' | 'Act' | 'Autonomous',
          snapshot_json: string(app.snapshot_json),
        }
      : null,
  };
}

export function decimal(value: unknown, max: bigint, signed = false): string {
  const text = string(value);
  if (
    !(signed ? /^-?[0-9]+$/ : /^[0-9]+$/).test(text) ||
    BigInt(text) > max ||
    BigInt(text) < (signed ? -max - 1n : 0n)
  )
    throw new Error('Invalid exact integer.');
  return text;
}
export function parseProfile(input: unknown) {
  const value = object(input);
  const kind = string(value.kind);
  if (kind !== 'Person' && kind !== 'Agent') throw new Error('Invalid identity kind.');
  return {
    id: publicKey(value.id),
    kind: kind as 'Person' | 'Agent',
    name: string(value.name),
    handle: string(value.handle),
    tint_hex: string(value.tint_hex),
    glyph: nullableText(value.glyph),
    bio: string(value.bio),
    owner: value.owner === null ? null : publicKey(value.owner),
    photo_sha256:
      value.photo_sha256 === undefined || value.photo_sha256 === null
        ? null
        : publicKey(value.photo_sha256),
    photo_ready: value.photo_ready === undefined ? false : boolean(value.photo_ready),
    photo_mime:
      value.photo_mime === undefined || value.photo_mime === null ? null : string(value.photo_mime),
  };
}
export type Profile = ReturnType<typeof parseProfile>;

export function parseConversation(input: unknown) {
  const value = object(input);
  const space = string(value.id);
  const kind = string(value.kind);
  if (kind !== 'direct' && kind !== 'group' && kind !== 'community' && kind !== 'personal')
    throw new Error('Invalid conversation kind.');
  const privacy = string(value.privacy);
  if (privacy !== 'EndToEnd' && privacy !== 'Closed' && privacy !== 'Public')
    throw new Error('Invalid conversation privacy.');
  const roster = Object.entries(object(value.roster)).map(([id, input]) => {
    const role = string(input);
    if (role !== 'Owner' && role !== 'Admin' && role !== 'Member' && role !== 'Reader')
      throw new Error('Invalid member role.');
    return { id: publicKey(id), role };
  });
  const prefs = object(value.prefs);
  return {
    id: string(value.id),
    title: string(value.title),
    kind,
    privacy: privacy as 'EndToEnd' | 'Closed' | 'Public',
    roster,
    profiles: array(value.profiles).map(parseProfile),
    prefs: {
      pinned: boolean(prefs.pinned),
      archived: boolean(prefs.archived),
      muted_until_ms:
        prefs.muted_until_ms === null
          ? null
          : decimal(prefs.muted_until_ms, 9223372036854775807n, true),
    },
    event_count: decimal(value.event_count, 18446744073709551615n),
    can_send: boolean(value.can_send),
    waiting: nullableText(value.waiting),
    integrity_error: nullableText(value.integrity_error ?? null),
    unreadable: integer(value.unreadable),
    unread: integer(value.unread ?? 0),
    mentions: integer(value.mentions ?? 0),
    members: integer(value.members ?? 0),
    messages: array(value.messages).map((message) => parseMessage(message, space)),
  };
}
export type Conversation = ReturnType<typeof parseConversation>;
export type Message = ReturnType<typeof parseMessage>;

export function parseDevices(input: unknown, current: string) {
  const response = object(input);
  if (response.kind !== 'devices') throw new Error('O servidor não retornou os dispositivos.');
  return array(response.devices).map((input) => {
    const device = object(input);
    const device_id = publicKey(device.device);
    return { device_id, revoked: boolean(device.revoked), this_device: device_id === current };
  });
}

export function parseInvite(input: unknown) {
  const value = object(input);
  if (value.kind !== 'preview') throw new Error('The relay did not return an invitation.');
  const role = string(value.role);
  if (role !== 'Owner' && role !== 'Admin' && role !== 'Member' && role !== 'Reader')
    throw new Error('Invalid invitation role.');
  return {
    space: string(value.space),
    title: string(value.title),
    role,
    members: integer(value.members),
  };
}
export type Invite = ReturnType<typeof parseInvite>;

export function parseUpdate(json: string) {
  const value = object(JSON.parse(json));
  return {
    snapshot: string(value.snapshot),
    frames: array(value.frames).map(bytes),
    conversations: array(value.conversations).map(parseConversation),
    maintenance: value.maintenance_ms == null ? null : integer(value.maintenance_ms),
    historyReady: value.history_ready === true,
    cursorRepair: parseCursorRepairStatus(value.cursor_repair),
  };
}

/** Core refusals arrive in English; people see plain Portuguese. */
export function plain(reason: string): string {
  if (/no longer available/i.test(reason)) return 'Essa mensagem não está mais disponível.';
  if (/foi apagada/i.test(reason)) return 'Essa mensagem foi apagada.';
  if (/Only the person/i.test(reason)) return 'Só quem enviou pode mudar esta mensagem.';
  if (/32 KB/i.test(reason)) return 'A mensagem não pode ficar vazia nem ser longa demais.';
  return 'Não deu para fazer isso agora. Tente de novo.';
}

export type MessagesState =
  | { kind: 'loading' }
  | {
      kind: 'active';
      connection: ConnectionState;
      conversations: Conversation[];
      notice: string | null;
    }
  | { kind: 'failed'; message: string; conversations: Conversation[] };
