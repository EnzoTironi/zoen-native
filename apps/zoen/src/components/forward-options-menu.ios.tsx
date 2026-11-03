import { Host } from '@expo/ui';
import { Menu, Section, Toggle } from '@expo/ui/swift-ui';
import {
  buttonStyle,
  disabled as disabledModifier,
  frame,
  labelStyle,
  tint,
} from '@expo/ui/swift-ui/modifiers';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { ForwardOptionsMenuProps } from './forward-options-menu.types';

export function ForwardOptionsMenu({
  options,
  canHideCaption,
  disabled = false,
  remaining = false,
  onChange,
}: ForwardOptionsMenuProps) {
  const { colors, scheme } = useTheme();
  const items = (
    <>
      <Toggle
        testID="forward-show-author"
        label={t('chat.showForwardAuthor')}
        systemImage="person.crop.circle"
        isOn={!options.hide_author}
        onIsOnChange={(show) => {
          if (!disabled) onChange({ ...options, hide_author: !show });
        }}
      />
      {canHideCaption ? (
        <Toggle
          testID="forward-show-caption"
          label={t('chat.showForwardCaption')}
          systemImage="text.bubble"
          isOn={!options.hide_caption}
          onIsOnChange={(show) => {
            if (!disabled) onChange({ ...options, hide_caption: !show });
          }}
        />
      ) : null}
    </>
  );
  return (
    <Host
      testID="forward-options-host"
      matchContents
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <Menu
        testID="forward-options-menu"
        label={t('chat.forwardOptions')}
        systemImage="slider.horizontal.3"
        modifiers={[
          buttonStyle('plain'),
          labelStyle('iconOnly'),
          frame({ minWidth: 44, minHeight: 44 }),
          tint(colors.accent),
          disabledModifier(disabled),
        ]}
      >
        {remaining ? <Section title={t('chat.remainingForwardMessages')}>{items}</Section> : items}
      </Menu>
    </Host>
  );
}
