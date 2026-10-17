import type { TimelineEntry } from './types';

export function messageText(entry: TimelineEntry): string {
  if (entry.deleted) return 'Mensagem apagada';
  if ('Message' in entry.kind) return entry.kind.Message.text;
  if ('System' in entry.kind) return entry.kind.System.text;
  if ('ItemEdited' in entry.kind) return entry.kind.ItemEdited.note;
  if ('Request' in entry.kind) return entry.kind.Request.title;
  return 'Aparência da conversa atualizada';
}

export function money(cents: string): string {
  const amount = BigInt(cents);
  const absolute = amount < 0n ? -amount : amount;
  return `${amount < 0n ? '−' : ''}R$ ${(absolute / 100n).toLocaleString('pt-BR')},${String(absolute % 100n).padStart(2, '0')}`;
}

/** Explicit pt-BR entry, parsed without floating-point rounding. */
export function parseMoneyInput(input: string): string {
  const value = input.trim();
  const match = /^(\d+|\d{1,3}(?:\.\d{3})+)(?:,(\d{1,2}))?$/.exec(value);
  if (!match) throw new Error('Use um valor em reais, como 100,00.');
  const cents =
    BigInt(match[1]!.replaceAll('.', '')) * 100n + BigInt((match[2] ?? '').padEnd(2, '0'));
  if (cents > 9223372036854775807n) throw new Error('Este valor é grande demais.');
  return String(cents);
}
