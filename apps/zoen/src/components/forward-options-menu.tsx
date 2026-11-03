import { MenuView } from '@expo/ui/community/menu';
import { View } from 'react-native';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { Icon } from './primitives';
import type { ForwardOptionsMenuProps } from './forward-options-menu.types';

export function ForwardOptionsMenu({
  options,
  canHideCaption,
  disabled = false,
  remaining = false,
  onChange,
}: ForwardOptionsMenuProps) {
  const { scheme, colors } = useTheme();
  return (
    <MenuView
      testID="forward-options-menu"
      colorScheme={scheme}
      title={remaining ? t('chat.remainingForwardMessages') : undefined}
      actions={[
        {
          id: 'author',
          title: t('chat.showForwardAuthor'),
          state: options.hide_author ? 'off' : 'on',
          attributes: { disabled },
        },
        ...(canHideCaption
          ? [
              {
                id: 'caption',
                title: t('chat.showForwardCaption'),
                state: options.hide_caption ? ('off' as const) : ('on' as const),
                attributes: { disabled },
              },
            ]
          : []),
      ]}
      onPressAction={({ nativeEvent }) => {
        if (disabled) return;
        if (nativeEvent.event === 'author')
          onChange({ ...options, hide_author: !options.hide_author });
        if (nativeEvent.event === 'caption' && canHideCaption)
          onChange({ ...options, hide_caption: !options.hide_caption });
      }}
    >
      <View
        accessible
        accessibilityRole="button"
        accessibilityLabel={t('chat.forwardOptions')}
        accessibilityState={{ disabled }}
        style={{ width: 44, height: 44, alignItems: 'center', justifyContent: 'center' }}
      >
        <Icon name="slider.horizontal.3" size={22} color={colors.accent} />
      </View>
    </MenuView>
  );
}
