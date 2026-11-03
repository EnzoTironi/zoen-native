export type DialogChoiceOptions = {
  title: string;
  choices: readonly { id: string; label: string; destructive?: boolean }[];
  signal?: AbortSignal;
};
export type DialogOptions = {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
};
export type Dialogs = {
  choose: (options: DialogChoiceOptions) => Promise<string | null>;
  confirm: (options: DialogOptions) => Promise<boolean>;
  inform: (title: string, message: string) => Promise<void>;
};
