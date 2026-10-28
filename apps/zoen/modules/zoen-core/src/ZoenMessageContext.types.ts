/** UI-only native capability. The caller still reauthorizes the signed current message. */
export type MessageContextAction =
  | 'reply'
  | 'thread'
  | 'copy'
  | 'select'
  | 'selectMessages'
  | 'edit'
  | 'recover'
  | 'forward'
  | 'share'
  | 'delete';
export type MessageContextEmoji = '👍' | '❤️' | '😂' | '😮' | '😢' | '🙏';
export type MessageContextSelection =
  | MessageContextAction
  | `reaction:${MessageContextEmoji}`
  | 'more'
  | `select:${string}`;
export type MessageContextTheme = Readonly<{
  background: string;
  highlight: string;
  text: string;
  secondary: string;
  separator: string;
  destructive: string;
  dim: string;
  isDark: boolean;
}>;
export type MessageContextConfiguration = Readonly<{
  actions: readonly Readonly<{ id: MessageContextAction; label: string }>[];
  reactions: readonly Readonly<{ emoji: MessageContextEmoji; label: string; selected: boolean }>[];
  moreLabel?: string;
  theme: MessageContextTheme;
  reducedMotion: boolean;
  incoming: boolean;
}>;
export type MessageContextNative = Readonly<{
  /** Completion is after host restoration AND native child-controller removal. */
  presentContext(
    requestId: string,
    binding: string,
    gestureId: string,
    configuration: string,
  ): Promise<string | null>;
  dismissContext(requestId: string): Promise<void>;
  finishTextSelection?(requestId: string, resultJSON: string | null): Promise<void>;
  consumeGesture(binding: string, gestureId: string): Promise<boolean>;
}>;
export type MessageContextGesture = Readonly<{
  binding: string;
  gestureId: string;
  sequence: number;
  kind: 'tap' | 'hold' | 'double' | 'selection';
  selection?: Readonly<{ requestId: string; active: boolean }>;
}>;
export type MessageContextRequest = Readonly<{
  binding: string;
  gestureId: string;
  kind: 'tap' | 'hold' | 'double' | 'accessibility';
  textSelection?: MessageTextSelectionBinding;
  /** Exact row and native ref, checked again after keyboard/removal awaits. */
  current(): boolean;
  native: MessageContextNative;
}>;

export type MessageTextSelectionHandler = Readonly<{
  started(): void;
  review(text: string, sourceRangesJSON: string): Promise<string>;
}>;
export type MessageTextSelectionBinding = Readonly<{
  bind(handler: MessageTextSelectionHandler): () => void;
}>;
