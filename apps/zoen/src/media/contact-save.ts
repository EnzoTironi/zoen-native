import type { ContactPreview } from './types';

// OS contact creation is enabled only on platforms with a qualified presenter.
export const saveContact:
  | ((contact: ContactPreview, cancelButtonTitle: string) => Promise<boolean>)
  | null = null;
