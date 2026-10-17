import { useSyncExternalStore } from 'react';
import { Text, View } from 'react-native';
import { NativeButton, Surface } from '../components/primitives';
import type { ItemDetail } from '../core/types';
import { useTheme } from '../theme';
import { activityContent } from './activity';
import type { SurfaceCoordinator } from './coordinator';
import { useNativeSurfaces } from './provider';
import { miniappSnapshot } from './snapshot';

function Controls({ coordinator, item }: { coordinator: SurfaceCoordinator; item: ItemDetail }) {
  const state = useSyncExternalStore(
    coordinator.subscribe,
    coordinator.getSnapshot,
    coordinator.getSnapshot,
  );
  const { colors } = useTheme();
  const preferences = state.preferences;
  if (!state.ready || !preferences) return null;
  const selected = preferences.widget?.itemId === item.id;
  const shortcut = preferences.shortcutItems.includes(item.id);
  const activity = preferences.activity?.itemId === item.id;
  const canStart = !!activityContent(item, preferences.owner);
  const run = (work: Promise<void>) => {
    void work.catch(() => {});
  };
  return (
    <Surface>
      <View style={{ gap: 12 }}>
        <Text style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}>No seu iPhone</Text>
        <Text style={{ color: colors.secondary }}>
          Escolha o que aparece fora do Zoen. O widget mantém o conteúdo oculto até você escolher
          mostrá-lo.
        </Text>
        <NativeButton
          secondary
          disabled={state.busy}
          label={selected ? 'Remover do widget' : 'Usar no widget'}
          onPress={() =>
            run(
              selected
                ? coordinator.removeWidget()
                : coordinator.selectWidget(item.id, item.version, false),
            )
          }
        />
        {selected ? (
          <NativeButton
            secondary
            disabled={state.busy}
            label={
              preferences.widget?.showContent
                ? 'Ocultar conteúdo do widget'
                : 'Mostrar conteúdo do widget'
            }
            onPress={() =>
              run(coordinator.selectWidget(item.id, item.version, !preferences.widget?.showContent))
            }
          />
        ) : null}
        <Text style={{ color: colors.secondary }}>
          Adicione “Miniapp do Zoen” pela galeria de widgets do iPhone. Esta seleção vale para os
          widgets do Zoen neste aparelho.
        </Text>
        <NativeButton
          secondary
          disabled={state.busy}
          label={shortcut ? 'Remover dos atalhos' : 'Disponibilizar nos Atalhos e Siri'}
          onPress={() => run(coordinator.selectShortcut(item.id, item.version, !shortcut))}
        />
        <Text style={{ color: colors.secondary }}>
          Os atalhos selecionados incluem o nome deste miniapp e da conversa para encontrar e abrir
          no Zoen.
        </Text>
        {activity || canStart ? (
          <NativeButton
            secondary
            disabled={state.busy}
            label={activity ? 'Encerrar contagem ao vivo' : 'Iniciar contagem ao vivo'}
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
