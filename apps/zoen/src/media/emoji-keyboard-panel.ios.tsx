import { useSyncExternalStore } from 'react';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { AttachmentKeyboardSession } from './attachment-keyboard-session';
import { NativeEmojiKeyboard, useKeyboardPanelBinding } from './keyboard-panel-native.ios';
export function EmojiKeyboardPanel({ session }: { session: AttachmentKeyboardSession }) {
  const { reducedMotion } = useTheme();
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const presentation = state.presentation?.kind === 'emoji' ? state.presentation : null;
  const binding = useKeyboardPanelBinding(
    presentation,
    !!presentation && !presentation.closing && state.enabled,
  );
  return presentation ? (
    <NativeEmojiKeyboard
      testID="composer-emoji-keyboard-host"
      style={{ position: 'absolute', width: 0, height: 0 }}
      binding={binding}
      model={JSON.stringify({
        reducedMotion,
        emojiLabels: {
          keyboard: t('composerKeyboard.keyboard'),
          delete: t('composerKeyboard.delete'),
          recent: t('composerKeyboard.recent'),
          variants: t('composerKeyboard.variants'),
          people: t('composerKeyboard.people'),
          animalsAndNature: t('composerKeyboard.animalsAndNature'),
          foodAndDrink: t('composerKeyboard.foodAndDrink'),
          activityAndSport: t('composerKeyboard.activityAndSport'),
          travelAndPlaces: t('composerKeyboard.travelAndPlaces'),
          objects: t('composerKeyboard.objects'),
          symbols: t('composerKeyboard.symbols'),
          flags: t('composerKeyboard.flags'),
        },
      })}
      onAction={({ nativeEvent: event }) => {
        if (
          event.token === presentation.id &&
          event.context === presentation.field.context &&
          event.action === 'keyboard' &&
          session.isCurrent(event.token)
        )
          session.closeKeyboardPanel();
      }}
      onClosed={({ nativeEvent: event }) => {
        if (event.token === presentation.id && event.context === presentation.field.context)
          if (session.owns(event.token)) {
            if (event.reason === 'closed') session.closed(event.token);
            else session.retire();
          }
      }}
    />
  ) : null;
}
