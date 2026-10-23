import { Host } from '@expo/ui';
import { Button, Menu } from '@expo/ui/swift-ui';
import { buttonStyle, frame, labelStyle } from '@expo/ui/swift-ui/modifiers';
import { router } from 'expo-router';
import { useClient } from '../core/provider';
import { homeOwnerFor } from '../home/access';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
export function WorkspaceCreate({ space }: { space: string }) {
  const state = useClient();
  const { colors, scheme } = useTheme();
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
          label={t('workspace.newPage')}
          systemImage="doc.badge.plus"
          onPress={() => router.push({ pathname: '/new-page', params: { space } })}
        />
        {homeOwnerFor(state)?.mode === 'account' ? (
          <Button
            label={t('sites.newSite')}
            systemImage="globe"
            onPress={() => router.push({ pathname: '/new-site', params: { space } })}
          />
        ) : null}
      </Menu>
    </Host>
  );
}
