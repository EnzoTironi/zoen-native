import { Host } from '@expo/ui';
import { Button, Menu } from '@expo/ui/swift-ui';
import { buttonStyle, frame, labelStyle } from '@expo/ui/swift-ui/modifiers';
import { router } from 'expo-router';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';

export function HomeActions() {
  const { scheme, colors } = useTheme();
  return (
    <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
      <Menu
        label={t('home.add')}
        systemImage="plus"
        modifiers={[
          buttonStyle('glass'),
          labelStyle('iconOnly'),
          frame({ minWidth: 44, minHeight: 44 }),
        ]}
      >
        <Button
          label={t('home.addMiniapp')}
          systemImage="square.grid.2x2"
          onPress={() => router.push('/miniapps')}
        />
        <Button
          label={t('home.newChat')}
          systemImage="square.and.pencil"
          onPress={() => router.push('/new-chat')}
        />
        <Button
          label={t('home.activity')}
          systemImage="square.stack"
          onPress={() => router.push('/activity')}
        />
        <Button
          label={t('home.search')}
          systemImage="magnifyingglass"
          onPress={() => router.push('/search')}
        />
      </Menu>
    </Host>
  );
}

export function HomeCardMenu({
  name,
  pinned,
  onToggle,
}: {
  name: string;
  pinned: boolean;
  onToggle: () => void;
}) {
  const { scheme } = useTheme();
  return (
    <Host matchContents colorScheme={scheme}>
      <Menu
        label={t('home.options', { name })}
        systemImage="ellipsis"
        modifiers={[
          buttonStyle('glass'),
          labelStyle('iconOnly'),
          frame({ minWidth: 44, minHeight: 44 }),
        ]}
      >
        <Button
          label={t(pinned ? 'home.unpin' : 'home.pin')}
          systemImage={pinned ? 'pin.slash' : 'pin'}
          onPress={onToggle}
        />
      </Menu>
    </Host>
  );
}
