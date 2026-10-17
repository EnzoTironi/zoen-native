import type { Message } from '../../../../packages/zoen-ui/web/messages';
import type { TimelineEntry } from './types';

type MediaMessage = Pick<Message, 'file' | 'text' | 'deleted'> & {
  file_item_id: string | null;
  file_version: number | null;
};

/** The owning signed Item/version is required; a filename is never an identity. */
export function browserMediaEntry(message: MediaMessage): Pick<TimelineEntry, 'kind' | 'voice'> {
  const file = message.deleted ? null : message.file;
  const itemId = message.file_item_id;
  const version = message.file_version;
  if (!file || !itemId || !version)
    return { kind: { Message: { text: message.text, card: null } }, voice: null };
  if (file.voice)
    return {
      kind: { Message: { text: message.text, card: null } },
      voice: {
        item_id: itemId,
        sha256: file.sha256,
        mime: file.mime,
        ms: String(file.voice.ms),
        levels: Array.from(file.voice.levels)
          .slice(0, 160)
          .filter((value) => /^[0-9a-f]$/i.test(value))
          .map((value) => parseInt(value, 16) / 15),
        transcript: file.voice.transcript,
      },
    };
  return {
    kind: {
      Message: {
        text: message.text,
        card: {
          item_id: itemId,
          title: file.name,
          summary: `${file.mime} · ${file.bytes} B`,
          kind_label: 'Arquivo',
          kind_id: 'file',
          version,
          total_cents: null,
          budget_cents: null,
          line_count: 0,
          done_count: 0,
          app: null,
        },
      },
    },
    voice: null,
  };
}
