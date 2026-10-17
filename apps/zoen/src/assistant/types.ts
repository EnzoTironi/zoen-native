export type AssistantUnavailableReason =
  | 'not_eligible'
  | 'disabled'
  | 'not_ready'
  | 'unsupported_locale'
  | 'unsupported';

export type AssistantOpen =
  | Readonly<{ available: true; id: string; contextMessages: number; limited: boolean }>
  | Readonly<{ available: false; reason: AssistantUnavailableReason }>;

/** Volatile, read-only on-device conversation. Closing discards its model transcript. */
export interface AssistantPort {
  open(spaceId: string, locale: 'en' | 'pt-BR'): Promise<AssistantOpen>;
  ask(id: string, question: string): Promise<Readonly<{ text: string }>>;
  close(id: string): Promise<void>;
}
