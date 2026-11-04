import { createContext } from 'react';
/** Retained chat owner handles navigation; entity text cannot select another chat. */
export const ConversationHashtagSearch = createContext<((tag: string) => void) | null>(null);
