import { Pressable, Text, View } from 'react-native';
import { router } from 'expo-router';
import { Icon } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { workspaceKind, type WorkspaceRow } from './workspace-index';

const symbols = {
  pages: { ios: 'doc.text', android: 'description', web: 'description' },
  slides: { ios: 'rectangle.on.rectangle', android: 'slideshow', web: 'slideshow' },
  sites: { ios: 'globe', android: 'language', web: 'language' },
  images: { ios: 'photo', android: 'image', web: 'image' },
  files: { ios: 'doc', android: 'draft', web: 'draft' },
  apps: { ios: 'square.grid.2x2', android: 'apps', web: 'apps' },
} as const;

export function WorkspaceItemRow({
  row,
  openFolder,
}: {
  row: WorkspaceRow;
  openFolder: (path: string) => void;
}) {
  const { colors, presentation: p } = useTheme();
  const folder = row.type === 'folder';
  const title = folder ? row.name : row.item.title;
  const detail = folder
    ? t('workspace.folderCount', { count: row.count })
    : [row.item.space_title, row.item.kind_label, row.item.path].filter(Boolean).join(' · ');
  return (
    <Pressable
      testID={`workspace-${row.key}`}
      accessibilityRole="button"
      accessibilityLabel={`${title}, ${detail}`}
      onPress={() =>
        folder
          ? openFolder(row.path)
          : router.push({ pathname: '/item/[id]', params: { id: row.item.id } })
      }
      style={({ pressed }) => ({
        flexDirection: 'row',
        alignItems: 'center',
        gap: 14,
        paddingVertical: 14,
        paddingHorizontal: 8,
        minHeight: 76,
        borderRadius: 14,
        borderCurve: 'continuous',
        backgroundColor: pressed ? p.list.highlighted : 'transparent',
      })}
    >
      <View
        style={{
          width: 44,
          height: 48,
          borderRadius: 12,
          backgroundColor: colors.muted,
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <Icon
          name={
            folder
              ? { ios: 'folder.fill', android: 'folder', web: 'folder' }
              : symbols[workspaceKind(row.item)]
          }
          color={colors.accent}
        />
      </View>
      <View style={{ flex: 1, gap: 5 }}>
        <Text numberOfLines={2} style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}>
          {title}
        </Text>
        <Text numberOfLines={2} style={{ color: colors.secondary, fontSize: 13 }}>
          {detail}
        </Text>
      </View>
      {folder ? (
        <Icon
          name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
          size={16}
          color={colors.tertiary}
        />
      ) : (
        <Text style={{ color: colors.tertiary, fontSize: 12, fontVariant: ['tabular-nums'] }}>
          v{row.item.version}
        </Text>
      )}
    </Pressable>
  );
}
