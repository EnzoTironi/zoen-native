import { createContext } from 'react';
/** Suppresses only duplicate physical bubble presses; VoiceOver actions remain native. */
export const MessageContextOwnsPress = createContext(false);
export const MessageContextStageEnabled = createContext(false);
export const MessageContextStageCommands = createContext<(() => Promise<void>) | null>(null);
export const MessageContextControlBarrier = createContext<(() => Promise<void>) | null>(null);
export type MessageContextStageHandle = Readonly<{
  retireContexts(): Promise<void>;
  dismissContexts(): Promise<void>;
}>;
export type {
  MessageContextConfiguration,
  MessageContextGesture,
  MessageContextNative,
  MessageContextRequest,
  MessageContextSelection,
  MessageContextTheme,
} from '../../modules/zoen-core/src/ZoenMessageContext.types';

/** Active only on the original source inside the existing native context controller. */
export const MessageTextSelection = createContext<{
  active: boolean;
  pending: boolean;
  error: boolean;
  quote(text: string, ranges?: string): Promise<void>;
} | null>(null);
