export type DialogOptions = {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
};
export type Dialogs = {
  confirm: (options: DialogOptions) => Promise<boolean>;
  inform: (title: string, message: string) => Promise<void>;
};
