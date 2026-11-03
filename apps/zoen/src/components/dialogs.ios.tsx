/* oxlint-disable jsx-a11y/aria-role -- SwiftUI Button roles use the native API, including cancel. */
import { createContext, use, useMemo, type ReactNode } from 'react';
import { Host } from '@expo/ui';
import { Button, ConfirmationDialog, VStack } from '@expo/ui/swift-ui';
import { frame } from '@expo/ui/swift-ui/modifiers';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { nativeDialogs } from './native-dialogs';
import type { Dialogs } from './dialog-types';
import { useDialogChoices } from './use-dialog-choices';

const Context = createContext<Dialogs | null>(null);

export function DialogsProvider({ children }: { children: ReactNode }) {
  const { scheme } = useTheme();
  const { pending, choose, finish } = useDialogChoices();
  const api = useMemo<Dialogs>(() => ({ ...nativeDialogs, choose }), [choose]);
  return (
    <Context value={api}>
      {children}
      {pending ? (
        <Host
          colorScheme={scheme}
          style={{ position: 'absolute', bottom: 0, left: '50%', width: 1, height: 1 }}
        >
          <ConfirmationDialog
            key={pending.id}
            title={pending.options.title}
            titleVisibility="visible"
            isPresented
            onIsPresentedChange={(visible) => {
              if (!visible) finish(pending.id, null);
            }}
          >
            <ConfirmationDialog.Trigger>
              <VStack modifiers={[frame({ width: 1, height: 1 })]}>{null}</VStack>
            </ConfirmationDialog.Trigger>
            <ConfirmationDialog.Actions>
              {pending.options.choices.map((choice) => (
                <Button
                  key={choice.id}
                  label={choice.label}
                  role={choice.destructive ? 'destructive' : undefined}
                  onPress={() => finish(pending.id, choice.id)}
                />
              ))}
              <Button
                label={t('common.cancel')}
                role="cancel"
                onPress={() => finish(pending.id, null)}
              />
            </ConfirmationDialog.Actions>
          </ConfirmationDialog>
        </Host>
      ) : null}
    </Context>
  );
}
export function useDialogs(): Dialogs {
  const context = use(Context);
  if (!context) throw new Error('DialogsProvider is missing.');
  return context;
}
