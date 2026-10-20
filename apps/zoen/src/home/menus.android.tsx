import { View } from 'react-native';
import { Host } from '@expo/ui';
import {
  DropdownMenu,
  DropdownMenuItem,
  FilledTonalIconButton,
  Icon,
  Text,
} from '@expo/ui/jetpack-compose';
import { semantics, size, testID as nativeTestID } from '@expo/ui/jetpack-compose/modifiers';
import { useLayoutEffect, useRef, useState } from 'react';
import { router } from 'expo-router';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { homeCardActions, type HomeCardAction } from './card-actions';
import type { HomeActionsProps, HomeCardMenuProps } from './menus.types';

export function HomeActions({ hiddenCount = 0, onRestoreHidden }: HomeActionsProps) {
  return (
    <View style={{ gap: 8, alignItems: 'flex-end' }}>
      <NativeButton label={t('home.addMiniapp')} onPress={() => router.push('/miniapps')} />
      {hiddenCount > 0 && onRestoreHidden ? (
        <NativeButton
          testID="home-restore-hidden-apps"
          label={t('home.restoreHiddenCount', { count: hiddenCount })}
          secondary
          onPress={onRestoreHidden}
        />
      ) : null}
    </View>
  );
}

export function HomeCardMenu(props: HomeCardMenuProps) {
  const { colors, scheme } = useTheme();
  const actions = homeCardActions(props);
  const scope = `${props.testID ?? props.name}:${actions.map((action) => action.id).join(',')}`;
  const [popup, setPopup] = useState({ scope, expanded: false });
  if (popup.scope !== scope) setPopup({ scope, expanded: false });
  const live = useRef({ scope, open: false });
  useLayoutEffect(() => {
    live.current = { scope, open: false };
    return () => {
      live.current.open = false;
    };
  }, [scope]);
  function dismiss() {
    if (live.current.scope !== scope) return;
    live.current.open = false;
    setPopup({ scope, expanded: false });
  }
  function select(action: HomeCardAction) {
    if (live.current.scope !== scope || !live.current.open) return;
    dismiss();
    action.onPress();
  }
  if (!actions.length) return null;
  return (
    <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
      <DropdownMenu
        expanded={popup.scope === scope && popup.expanded}
        onDismissRequest={dismiss}
        color={colors.raised}
        properties={{
          focusable: true,
          dismissOnBackPress: true,
          dismissOnClickOutside: true,
          clippingEnabled: false,
        }}
      >
        <DropdownMenu.Trigger>
          <FilledTonalIconButton
            modifiers={[
              size(48, 48),
              semantics({ contentDescription: t('home.options', { name: props.name }) }),
              ...(props.testID ? [nativeTestID(props.testID)] : []),
            ]}
            onClick={() => {
              if (live.current.scope !== scope || live.current.open) return;
              live.current.open = true;
              setPopup({ scope, expanded: true });
            }}
          >
            <Icon source={require('../../assets/icons/more-horizontal.xml')} size={24} />
          </FilledTonalIconButton>
        </DropdownMenu.Trigger>
        <DropdownMenu.Items>
          {actions.map((action) => (
            <DropdownMenuItem key={action.id} onClick={() => select(action)}>
              <DropdownMenuItem.Text>
                <Text>{action.label}</Text>
              </DropdownMenuItem.Text>
            </DropdownMenuItem>
          ))}
        </DropdownMenu.Items>
      </DropdownMenu>
    </Host>
  );
}
