import { Host } from '@expo/ui';
import { Button, HStack, TextField } from '@expo/ui/swift-ui';
import {
  accessibilityLabel,
  autocorrectionDisabled,
  buttonStyle,
  font,
  foregroundStyle,
  frame,
  labelStyle,
  submitLabel,
  onSubmit,
  textInputAutocapitalization,
} from '@expo/ui/swift-ui/modifiers';
import { useWindowDimensions } from 'react-native';
import { conversationQueryFits } from '../core/conversation-search';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { ConversationSearchProps } from './conversation-search.types';

export function ConversationSearch({ state, onChange, onSearch, onAsk }: ConversationSearchProps) {
  const { colors, scheme } = useTheme();
  const { width } = useWindowDimensions();
  const canAsk =
    state.completed &&
    !state.loading &&
    !state.error &&
    state.hits.length === 0 &&
    !state.cursor &&
    Boolean(state.query.trim()) &&
    conversationQueryFits(state.query.trim());
  return (
    <Host style={{ width: Math.max(100, width - 200), height: 44 }} colorScheme={scheme}>
      <HStack spacing={0}>
        <Button
          systemImage={canAsk ? 'sparkles' : 'magnifyingglass'}
          label={t(canAsk ? 'chat.askAboutChat' : 'chat.searchThisChat')}
          onPress={canAsk ? onAsk : onSearch}
          modifiers={[
            buttonStyle('plain'),
            labelStyle('iconOnly'),
            font({ size: 18 }),
            foregroundStyle(canAsk ? colors.accent : colors.text),
            frame({ width: 44, height: 44 }),
          ]}
        />
        <TextField
          autoFocus
          maxLength={512}
          placeholder={t('chat.searchThisChat')}
          onTextChange={onChange}
          modifiers={[
            foregroundStyle(colors.text),
            font({ size: 16 }),
            accessibilityLabel(t('chat.searchThisChat')),
            autocorrectionDisabled(),
            textInputAutocapitalization('never'),
            submitLabel('search'),
            onSubmit(onSearch),
          ]}
        />
      </HStack>
    </Host>
  );
}
