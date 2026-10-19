import { t } from '../i18n/index.ts';
import { useSyncExternalStore } from 'react';
import { Text, View } from 'react-native';
import { NativeButton, Surface } from '../components/primitives';
import type { ItemDetail } from '../core/types';
import { useTheme } from '../theme';
import { activityContent } from './activity';
import type { SurfaceCoordinator } from './coordinator';
import { useNativeSurfaces } from './provider';
import { miniappSnapshot } from './snapshot';
import { privateWidgetTitle } from './widget-catalog';

function Controls({ coordinator, item }: { coordinator: SurfaceCoordinator; item: ItemDetail }) {
  const state = useSyncExternalStore(
    coordinator.subscribe,
    coordinator.getSnapshot,
    coordinator.getSnapshot,
  );
  const { colors } = useTheme();
  const preferences = state.preferences;
  if (!state.ready || !preferences) return null;
  const widgetIndex = preferences.widgets.findIndex((selection) => selection.itemId === item.id);
  const widget = preferences.widgets[widgetIndex];
  const selected = !!widget;
  const shortcut = preferences.shortcutItems.includes(item.id);
  const activity = preferences.activity?.itemId === item.id;
  const canStart = !!activityContent(item, preferences.owner);
  const run = (work: Promise<void>) => {
    void work.catch(() => {});
  };
  return (
    <Surface>
      <View style={{ gap: 12 }}>
        <Text style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}>
          {t('nativeSurfaces.onYourIPhone')}
        </Text>
        <Text style={{ color: colors.secondary }}>
          {t('nativeSurfaces.chooseWhatAppearsOutsideZoenTheWidgetKeeps')}
        </Text>
        <NativeButton
          secondary
          disabled={state.busy}
          label={selected ? t('nativeSurfaces.removeFromWidget') : t('nativeSurfaces.useInWidget')}
          onPress={() =>
            run(
              selected
                ? coordinator.removeWidget(item.id)
                : coordinator.selectWidget(item.id, item.version, false),
            )
          }
        />
        {selected ? (
          <NativeButton
            secondary
            disabled={state.busy}
            label={
              widget?.showContent
                ? t('nativeSurfaces.hideWidgetContent')
                : t('nativeSurfaces.showWidgetContent')
            }
            onPress={() =>
              run(coordinator.selectWidget(item.id, item.version, !widget?.showContent))
            }
          />
        ) : null}
        {widget && !widget.showContent ? (
          <Text selectable style={{ color: colors.secondary }}>
            {t('nativeSurfaces.privateWidgetExplanation', {
              name: privateWidgetTitle(widgetIndex + 1),
            })}
          </Text>
        ) : null}
        <Text style={{ color: colors.secondary }}>
          {t('nativeSurfaces.addZoenMiniappFromTheIPhoneWidgetGallery')}
        </Text>
        <NativeButton
          secondary
          disabled={state.busy}
          label={
            shortcut
              ? t('nativeSurfaces.removeFromShortcuts')
              : t('nativeSurfaces.makeAvailableInShortcutsAndSiri')
          }
          onPress={() => run(coordinator.selectShortcut(item.id, item.version, !shortcut))}
        />
        <Text style={{ color: colors.secondary }}>
          {t('nativeSurfaces.selectedShortcutsIncludeTheMiniappAndChatNames')}
        </Text>
        {activity || canStart ? (
          <NativeButton
            secondary
            disabled={state.busy}
            label={
              activity
                ? t('nativeSurfaces.endLiveCountdown')
                : t('nativeSurfaces.startLiveCountdown')
            }
            onPress={() =>
              run(
                activity
                  ? coordinator.stopActivity()
                  : coordinator.startActivity(item.id, item.version, false),
              )
            }
          />
        ) : null}
        {state.error ? (
          <Text accessibilityRole="alert" style={{ color: colors.danger }}>
            {state.error}
          </Text>
        ) : null}
      </View>
    </Surface>
  );
}
export function MiniappSurfaceControls({ item }: { item: ItemDetail }) {
  const coordinator = useNativeSurfaces();
  return coordinator && miniappSnapshot(item) ? (
    <Controls coordinator={coordinator} item={item} />
  ) : null;
}
