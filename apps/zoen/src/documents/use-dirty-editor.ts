import { t } from '../i18n/index.ts';
import { useRef, useState } from 'react';
import { usePreventRemove } from 'expo-router';
import { useDialogs } from '../components/dialogs';

/** SDK 58 also protects browser reloads and closing the tab. */
export function useDirtyEditor(dirty: boolean, busy: boolean) {
  const dialogs = useDialogs();
  const [leaving, setLeaving] = useState(false);
  const confirming = useRef(false);
  const disable = usePreventRemove(!leaving && (dirty || busy), ({ repeat }) => {
    if (confirming.current) return;
    if (busy) {
      void dialogs.inform(
        t('documents.savingChanges'),
        t('documents.waitForThisOperationToFinishBeforeLeaving'),
      );
      return;
    }
    confirming.current = true;
    void dialogs
      .confirm({
        title: t('documents.leaveWithoutSaving'),
        message: t('documents.changesInThisEditorHaveNotBeenSaved'),
        confirmLabel: t('documents.discardChanges'),
        cancelLabel: t('documents.keepEditing'),
        destructive: true,
      })
      .then((accepted) => {
        if (accepted) {
          setLeaving(true);
          repeat();
        }
      })
      .finally(() => {
        confirming.current = false;
      });
  });
  return () => {
    setLeaving(true);
    disable();
  };
}
