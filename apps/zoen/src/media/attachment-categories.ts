/** Closed routes added to the existing attachment owner, never a provider registry. */
export const conversationAttachmentCategories = ['location', 'contact', 'article', 'poll'] as const;
export type ConversationAttachmentCategory = (typeof conversationAttachmentCategories)[number];
export function conversationAttachmentCategory(
  value: unknown,
): ConversationAttachmentCategory | null {
  return conversationAttachmentCategories.find((kind) => kind === value) ?? null;
}
