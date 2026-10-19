import { Host } from '@expo/ui';
import { Button, Text as NativeText } from '@expo/ui/swift-ui';
import {
  accessibilityLabel,
  background,
  buttonStyle,
  disabled as disabledModifier,
  foregroundStyle,
  frame,
  padding,
  strikethrough,
  fixedSize,
  multilineTextAlignment,
  cornerRadius,
} from '@expo/ui/swift-ui/modifiers';
import { useMemo, useRef, useState } from 'react';
import { Text, View, useWindowDimensions } from 'react-native';
import { GestureDetector, usePanGesture } from 'react-native-gesture-handler';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { projectVoiceDraft, type VoiceDraft, type VoiceEditCommand } from './voice-edit.ts';

type Props = {
  draft: VoiceDraft;
  onEdit: (commands: VoiceEditCommand | readonly VoiceEditCommand[]) => void;
  disabled?: boolean;
};
type Rect = { x: number; y: number; width: number; height: number };
type WordDrag = {
  draft: VoiceDraft;
  page: number;
  first: number;
  last: number;
  remove: boolean;
  layoutVersion: number;
};
const WORDS_PER_PAGE = 100;

/** Recognizer fragments may be phrases: preserve their original IDs and exact ranges. */
export function VoiceWords({ draft, onEdit, disabled = false }: Props) {
  const { colors, scheme } = useTheme();
  const [pagination, setPagination] = useState({ source: draft.original.id, page: 0 });
  const [preview, setPreview] = useState<WordDrag | null>(null);
  const { fontScale, width: windowWidth } = useWindowDimensions();
  const [width, setWidth] = useState(0);
  const layoutVersion = useRef(0);
  const layouts = useRef(new Map<string, Rect>());
  const dragging = useRef<WordDrag | null>(null);
  const words = draft.transcript?.words ?? [];
  const pages = Math.max(1, Math.ceil(words.length / WORDS_PER_PAGE));
  const page = pagination.source === draft.original.id ? Math.min(pages - 1, pagination.page) : 0;
  const pageWords = words.slice(page * WORDS_PER_PAGE, (page + 1) * WORDS_PER_PAGE);
  const kept = useMemo(
    () => new Set(projectVoiceDraft(draft).words.map((word) => word.id)),
    [draft],
  );
  const active = preview?.draft === draft && preview.page === page ? preview : null;
  const layoutKey = (wordId: string) => `${draft.original.id}:${page}:${wordId}`;
  function wordAt(x: number, y: number, nearest: boolean) {
    let closest: number | null = null;
    let distance = Infinity;
    pageWords.forEach((word, index) => {
      const rect = layouts.current.get(layoutKey(word.id));
      if (!rect) return;
      const dx = Math.max(rect.x - x, 0, x - rect.x - rect.width);
      const dy = Math.max(rect.y - y, 0, y - rect.y - rect.height);
      const squared = dx * dx + dy * dy;
      if ((nearest || squared === 0) && squared < distance) {
        closest = index;
        distance = squared;
      }
    });
    return closest;
  }
  const pan = usePanGesture({
    enabled: !disabled && pageWords.length > 0,
    runOnJS: true,
    activateAfterLongPress: 300,
    cancelsTouchesInView: true,
    onBegin: (event) => {
      const first = wordAt(event.x, event.y, false);
      dragging.current =
        first === null
          ? null
          : {
              draft,
              page,
              first,
              last: first,
              remove: kept.has(pageWords[first]!.id),
              layoutVersion: layoutVersion.current,
            };
    },
    onActivate: () => {
      if (
        !disabled &&
        dragging.current?.draft === draft &&
        dragging.current.page === page &&
        dragging.current.layoutVersion === layoutVersion.current
      )
        setPreview(dragging.current);
    },
    onUpdate: (event) => {
      const context = dragging.current;
      if (
        disabled ||
        context?.draft !== draft ||
        context.page !== page ||
        context.layoutVersion !== layoutVersion.current
      )
        return;
      const last = wordAt(event.x, event.y, true);
      if (last === null) return;
      dragging.current = { ...context, last };
      setPreview(dragging.current);
    },
    onDeactivate: (event) => {
      const context = dragging.current;
      dragging.current = null;
      setPreview(null);
      if (
        event.canceled ||
        disabled ||
        context?.draft !== draft ||
        context.page !== page ||
        context.layoutVersion !== layoutVersion.current
      )
        return;
      const last = wordAt(event.x, event.y, true) ?? context.last;
      const firstIndex = Math.min(context.first, last);
      const lastIndex = Math.max(context.first, last);
      const commands: VoiceEditCommand[] = pageWords
        .slice(firstIndex, lastIndex + 1)
        .filter((word) => kept.has(word.id) === context.remove)
        .map((word) => ({ type: 'setWordRemoved', wordId: word.id, removed: context.remove }));
      if (commands.length) onEdit(commands);
    },
    onFinalize: () => {
      dragging.current = null;
      setPreview(null);
    },
  });
  function navigate(next: number) {
    if (disabled) return;
    dragging.current = null;
    setPreview(null);
    layouts.current.clear();
    setPagination({ source: draft.original.id, page: Math.max(0, Math.min(pages - 1, next)) });
  }
  if (!words.length) return null;
  return (
    <View style={{ gap: 12 }}>
      <Text style={{ color: colors.secondary, fontSize: 13 }}>{t('media.voiceWordsHint')}</Text>
      <GestureDetector gesture={pan}>
        <View
          testID="voice-words"
          collapsable={false}
          onLayout={({ nativeEvent }) => {
            const next = nativeEvent.layout.width;
            if (next !== width) {
              ++layoutVersion.current;
              setWidth(next);
            }
          }}
          style={{ flexDirection: 'row', flexWrap: 'wrap', alignItems: 'flex-start', gap: 4 }}
        >
          {pageWords.map((word, index) => {
            const selected =
              !!active &&
              Math.min(active.first, active.last) <= index &&
              index <= Math.max(active.first, active.last);
            const removed = selected ? active.remove : !kept.has(word.id);
            return (
              <View
                key={layoutKey(word.id)}
                onLayout={(event) => {
                  const key = layoutKey(word.id);
                  const next = event.nativeEvent.layout;
                  const previous = layouts.current.get(key);
                  if (
                    !previous ||
                    previous.x !== next.x ||
                    previous.y !== next.y ||
                    previous.width !== next.width ||
                    previous.height !== next.height
                  )
                    ++layoutVersion.current;
                  layouts.current.set(key, next);
                }}
                style={{ maxWidth: '100%' }}
              >
                <Host matchContents colorScheme={scheme}>
                  <Button
                    onPress={() => {
                      if (!disabled)
                        onEdit({
                          type: 'setWordRemoved',
                          wordId: word.id,
                          removed: kept.has(word.id),
                        });
                    }}
                    modifiers={[
                      buttonStyle('plain'),
                      disabledModifier(disabled),
                      accessibilityLabel(
                        t(removed ? 'media.voiceRestoreWord' : 'media.voiceRemoveWord', {
                          word: word.text,
                        }),
                      ),
                    ]}
                  >
                    <NativeText
                      modifiers={[
                        foregroundStyle(removed ? colors.secondary : colors.text),
                        strikethrough({ isActive: removed, pattern: 'solid' }),
                        padding({ horizontal: 7, vertical: 5 }),
                        fixedSize({ horizontal: false, vertical: true }),
                        frame({
                          maxWidth: Math.max(44, width || windowWidth - 48),
                          minWidth: 44,
                          minHeight: 44,
                          alignment: 'leading',
                        }),
                        multilineTextAlignment('leading'),
                        background(selected || removed ? colors.muted : 'transparent'),
                        cornerRadius(10),
                      ]}
                    >
                      {word.text}
                    </NativeText>
                  </Button>
                </Host>
              </View>
            );
          })}
        </View>
      </GestureDetector>
      {pages > 1 ? (
        <View
          style={{
            flexDirection: fontScale > 1.3 ? 'column' : 'row',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 12,
          }}
        >
          <Host matchContents colorScheme={scheme}>
            <Button
              label={t('media.voiceWordsPrevious')}
              onPress={() => navigate(page - 1)}
              modifiers={[
                frame({ minWidth: 44, minHeight: 44 }),
                disabledModifier(disabled || page === 0),
              ]}
            />
          </Host>
          <Text style={{ color: colors.secondary, flexShrink: 1 }}>
            {t('media.voiceWordsPage', { page: page + 1, total: pages })}
          </Text>
          <Host matchContents colorScheme={scheme}>
            <Button
              label={t('media.voiceWordsNext')}
              onPress={() => navigate(page + 1)}
              modifiers={[
                frame({ minWidth: 44, minHeight: 44 }),
                disabledModifier(disabled || page + 1 === pages),
              ]}
            />
          </Host>
        </View>
      ) : null}
    </View>
  );
}
