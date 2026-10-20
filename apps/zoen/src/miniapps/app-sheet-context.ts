import { createContext, useContext } from 'react';

/** A floating host control, measured from the top and right of the full App frame. */
export type AppCloseButton = Readonly<{
  top: number;
  right: number;
  width: number;
  height: number;
}>;
export const AppSheetCloseButtonContext = createContext<AppCloseButton | null>(null);
export const useAppSheetCloseButton = () => useContext(AppSheetCloseButtonContext);
