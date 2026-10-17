import type { ReactNode } from 'react';

export type MediaSheetProps = {
  visible: boolean;
  locked: boolean;
  onDismiss: () => void;
  children: ReactNode;
};
