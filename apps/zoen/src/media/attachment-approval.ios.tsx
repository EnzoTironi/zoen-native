import { Host } from '@expo/ui';
import { Button, Menu, TextField, useNativeState } from '@expo/ui/swift-ui';
import {
  accessibilityLabel,
  buttonBorderShape,
  buttonStyle,
  controlSize,
  disabled,
  font,
  foregroundStyle,
  frame,
  labelStyle,
  lineLimit,
  tint,
} from '@expo/ui/swift-ui/modifiers';
import { useEvent, useEventListener } from 'expo';
import { Image } from 'expo-image';
import { isLiquidGlassAvailable } from 'expo-glass-effect';
import { StatusBar } from 'expo-status-bar';
import * as Haptics from 'expo-haptics';
import { useVideoPlayer, VideoView } from 'expo-video';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Keyboard,
  Modal,
  Pressable,
  ScrollView,
  Text,
  View,
  useWindowDimensions,
  type ScrollViewInstance,
} from 'react-native';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { SafeAreaProvider, SafeAreaView, useSafeAreaInsets } from 'react-native-safe-area-context';
import { Icon } from '../components/primitives';
import type { AttachmentComposerField } from './attachment-composer';
import { DraftThumbnail } from './attachment-draft-strip';
import { AttachmentGalleryPager } from './attachment-gallery-pager';
import type { AttachmentApprovalProps } from './attachment-approval.types';
import { mediaPlayback, type PlaybackOwner } from './playback-owner';
import { t } from '../i18n';
import { useTheme } from '../theme';

export const supportsMediaApproval = true;
const background = '#000000';
const surface = '#1C1E22';
const secondary = '#A9ADB6';

function ApprovalAction({
  name,
  label,
  onPress,
  disabled: unavailable = false,
  prominent = false,
  testID,
}: {
  name: 'chevron.left' | 'arrow.up' | 'pencil.tip.crop.circle' | 'trash';
  label: string;
  onPress(): void;
  disabled?: boolean;
  prominent?: boolean;
  testID: string;
}) {
  const { colors } = useTheme();
  return (
    <Host
      style={{ width: prominent ? 46 : 44, height: 44 }}
      colorScheme="dark"
      ignoreSafeArea="keyboard"
    >
      <Button
        testID={testID}
        label={label}
        systemImage={name}
        onPress={onPress}
        modifiers={[
          buttonStyle(
            prominent ? 'borderedProminent' : isLiquidGlassAvailable() ? 'glass' : 'bordered',
          ),
          buttonBorderShape('circle'),
          controlSize('regular'),
          labelStyle('iconOnly'),
          tint(prominent ? colors.accent : '#FFFFFF'),
          foregroundStyle(prominent ? colors.onAccent : '#FFFFFF'),
          disabled(unavailable),
          frame({ width: prominent ? 46 : 44, height: 44 }),
        ]}
      />
    </Host>
  );
}

function Caption({
  field,
  onFocus,
}: {
  field: AttachmentComposerField;
  onFocus(value: boolean): void;
}) {
  const text = useNativeState(field.value);
  useLayoutEffect(() => {
    text.set(field.value);
  }, [text, field.value]);
  return (
    <Host
      matchContents={{ vertical: true }}
      style={{ flex: 1, minHeight: 20 }}
      colorScheme="dark"
      ignoreSafeArea="keyboard"
    >
      <TextField
        testID="attachment-approval-caption"
        text={text}
        axis="vertical"
        placeholder={t('media.attachmentCaptionPlaceholder')}
        onTextChange={field.onChange}
        onFocusChange={onFocus}
        modifiers={[
          font({ size: 17 }),
          lineLimit({ min: 1, max: 4 }),
          foregroundStyle('#FFFFFF'),
          disabled(!field.editable),
          accessibilityLabel(
            t('media.attachmentCaption', { position: field.position, total: field.total }),
          ),
        ]}
      />
    </Host>
  );
}

/** One selected preview player; content and its file lease stay with the channel owner. */
function ApprovalVideo({
  uri,
  active,
  isCurrent,
}: {
  uri: string;
  active: boolean;
  isCurrent(): boolean;
}) {
  const player = useVideoPlayer({ uri, useCaching: false }, (value) => {
    value.allowsExternalPlayback = false;
    value.staysActiveInBackground = false;
  });
  const playback = useRef<PlaybackOwner | null>(null);
  useEffect(() => {
    const owner = mediaPlayback.register(
      () => player.pause(),
      () => player.playing,
    );
    playback.current = owner;
    return () => {
      playback.current = null;
      owner.dispose();
    };
  }, [player]);
  useEventListener(player, 'playingChange', ({ isPlaying }) => {
    if (!active || !isCurrent()) playback.current?.interrupt();
    else playback.current?.observe(isPlaying);
  });
  useEffect(() => {
    if (!active) playback.current?.interrupt();
  }, [active]);
  const status = useEvent(player, 'statusChange', { status: player.status, error: undefined });
  return (
    <View
      style={{ flex: 1 }}
      onTouchStart={() => {
        if (active && isCurrent()) playback.current?.refresh();
        else playback.current?.interrupt();
      }}
    >
      <VideoView
        testID="attachment-approval-video"
        player={player}
        style={{ flex: 1 }}
        nativeControls
        contentFit="contain"
        playsInline
        allowsPictureInPicture={false}
        fullscreenOptions={{ enable: false }}
      />
      {status.status === 'loading' ? (
        <View
          pointerEvents="none"
          style={{ position: 'absolute', inset: 0, justifyContent: 'center' }}
        >
          <ActivityIndicator color="#FFFFFF" />
        </View>
      ) : null}
      {status.status === 'error' ? (
        <Text accessibilityRole="alert" style={{ color: secondary, padding: 16 }}>
          {t('media.thisVideoCouldNotBePlayed')}
        </Text>
      ) : null}
    </View>
  );
}

/** Telegram's selected-media rail uses screen-specific square thumbnails, with a four-point gap. */
function reviewThumbnailSide(width: number, height: number, scale: number) {
  if (scale < 2) return 106;
  const longest = Math.max(width, height);
  const metric = [
    [956, 145, 1],
    [926, 141, 1],
    [896, 137, -1],
    [874, 133, -1],
    [844, 129, -1],
    [812, 124, -1],
    [736, 137, -1],
    [667, 124, -1],
    [0, 106, -1],
  ].find(([minimum]) => longest >= minimum!)!;
  return metric[1]! + metric[2]! / scale;
}

function ApprovalContent(props: AttachmentApprovalProps) {
  const { state, field, selectedId, recipient, busy, editing, canAdd, canEdit, error } = props;
  const selected = state.items.find((item) => item.mutationId === selectedId) ?? state.items[0];
  const { bottom } = useSafeAreaInsets();
  const { width, height, scale } = useWindowDimensions();
  const { reducedMotion } = useTheme();
  const [typing, setTyping] = useState(false);
  const [showSelection, setShowSelection] = useState(false);
  const rail = useRef<ScrollViewInstance | null>(null);
  const position = Math.max(
    0,
    state.items.findIndex((item) => item === selected),
  );
  const thumbnailSide = reviewThumbnailSide(width, height, scale);
  useEffect(() => {
    if (showSelection)
      rail.current?.scrollTo({
        x: Math.max(0, position * (thumbnailSide + 4) - 4),
        animated: !reducedMotion,
      });
  }, [position, showSelection, thumbnailSide, reducedMotion]);
  if (!selected) return null;
  const select = (id: string) => {
    if (busy || !props.isCurrent() || id === selected.mutationId) return;
    void Haptics.selectionAsync().catch(() => {});
    props.onSelect(id);
  };
  return (
    <SafeAreaView edges={['left', 'right']} style={{ flex: 1, backgroundColor: background }}>
      <KeyboardAvoidingView behavior="padding" automaticOffset style={{ flex: 1 }}>
        <View testID="attachment-approval-stage" style={{ position: 'absolute', inset: 0 }}>
          <AttachmentGalleryPager
            items={state.items}
            selectedId={selected.mutationId}
            disabled={busy || editing || typing}
            reducedMotion={reducedMotion}
            isCurrent={props.isCurrent}
            onSelect={select}
            onDragStart={() => {
              Keyboard.dismiss();
              mediaPlayback.pauseAll();
            }}
            renderItem={(item, active) =>
              !item.uri ? (
                <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', gap: 12 }}>
                  <ActivityIndicator color="#FFFFFF" />
                  <Text style={{ color: secondary }}>{t('media.attachmentPreparing')}</Text>
                </View>
              ) : item.mime.startsWith('image/') ? (
                <Image
                  testID={`attachment-approval-image-${item.mutationId}`}
                  source={{ uri: item.uri }}
                  recyclingKey={item.mutationId}
                  cachePolicy="none"
                  contentFit="contain"
                  accessibilityLabel={item.name}
                  style={{ flex: 1 }}
                />
              ) : item.mime.startsWith('video/') && active ? (
                <ApprovalVideo
                  key={item.mutationId}
                  uri={item.uri}
                  active={!editing && !busy}
                  isCurrent={props.isCurrent}
                />
              ) : (
                <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', gap: 12 }}>
                  <Icon
                    name={
                      item.mime.startsWith('video/')
                        ? { ios: 'video', android: 'videocam', web: 'videocam' }
                        : { ios: 'doc', android: 'description', web: 'description' }
                    }
                    color="#FFFFFF"
                    size={42}
                  />
                  <Text style={{ color: '#FFFFFF', paddingHorizontal: 24 }} numberOfLines={2}>
                    {item.name}
                  </Text>
                </View>
              )
            }
          />
        </View>
        <View pointerEvents="box-none" style={{ flex: 1, justifyContent: 'flex-end' }}>
          <View style={{ paddingBottom: typing ? 8 : bottom, gap: 10 }}>
            {!typing ? (
              <>
                {showSelection ? (
                  <ScrollView
                    ref={rail}
                    testID="attachment-approval-selection-rail"
                    horizontal
                    showsHorizontalScrollIndicator={false}
                    keyboardShouldPersistTaps="handled"
                    contentContainerStyle={{ gap: 4, paddingHorizontal: 4, paddingVertical: 4 }}
                  >
                    {state.items.map((item, index) => (
                      <Pressable
                        key={item.mutationId}
                        testID={`attachment-approval-thumbnail-${index}`}
                        accessibilityRole="button"
                        accessibilityState={{ selected: item === selected, disabled: busy }}
                        accessibilityLabel={t('media.attachmentPosition', {
                          position: index + 1,
                          total: state.items.length,
                          name: item.name,
                        })}
                        disabled={busy}
                        onPress={() => select(item.mutationId)}
                        style={{
                          width: thumbnailSide,
                          height: thumbnailSide,
                          borderWidth: 2,
                          borderColor: item === selected ? '#FFFFFF' : 'transparent',
                        }}
                      >
                        <DraftThumbnail item={item} borderRadius={0} />
                      </Pressable>
                    ))}
                  </ScrollView>
                ) : null}
                <View style={{ alignItems: 'flex-end', paddingHorizontal: 8 }}>
                  <Host
                    style={{ width: 64, height: 38 }}
                    colorScheme="dark"
                    ignoreSafeArea="keyboard"
                  >
                    <Button
                      testID="attachment-approval-selection-count"
                      label={String(state.items.length)}
                      systemImage="photo.on.rectangle"
                      onPress={() => {
                        if (!busy && props.isCurrent()) setShowSelection((value) => !value);
                      }}
                      modifiers={[
                        buttonStyle(isLiquidGlassAvailable() ? 'glass' : 'bordered'),
                        font({ size: 15, weight: 'semibold' }),
                        foregroundStyle('#FFFFFF'),
                        disabled(busy),
                        frame({ width: 64, height: 38 }),
                        accessibilityLabel(
                          t('media.attachmentDraftCount', { count: state.items.length }),
                        ),
                      ]}
                    />
                  </Host>
                </View>
              </>
            ) : null}
            <View style={{ paddingHorizontal: 8, gap: 8 }}>
              {error ? (
                <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
                  <Text
                    accessibilityRole="alert"
                    style={{ flex: 1, color: secondary, fontSize: 12 }}
                  >
                    {error}
                  </Text>
                  <Host matchContents colorScheme="dark">
                    <Button
                      label={t('chat.tryAgain2')}
                      onPress={props.onRetry}
                      modifiers={[disabled(editing)]}
                    />
                  </Host>
                </View>
              ) : null}
              {field.retry ? (
                <Text style={{ color: secondary, fontSize: 12 }}>
                  {t('media.attachmentRetryLocked')}
                </Text>
              ) : null}
              <View
                style={{
                  minHeight: 40,
                  paddingHorizontal: 12,
                  paddingVertical: 10,
                  borderRadius: 20,
                  backgroundColor: surface,
                }}
              >
                <Caption key={selected.mutationId} field={field} onFocus={setTyping} />
              </View>
            </View>
            <View
              style={{
                flexDirection: 'row',
                minHeight: 44,
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 4,
                paddingHorizontal: 8,
              }}
            >
              <ApprovalAction
                testID="attachment-approval-close"
                name="chevron.left"
                label={t('common.close')}
                onPress={props.onClose}
              />
              {!typing ? (
                <View style={{ flexDirection: 'row', alignItems: 'center', gap: 4 }}>
                  {canEdit ? (
                    <ApprovalAction
                      testID="attachment-approval-edit"
                      name="pencil.tip.crop.circle"
                      label={t('media.attachmentEdit')}
                      disabled={busy}
                      onPress={() => {
                        Keyboard.dismiss();
                        props.onEdit();
                      }}
                    />
                  ) : null}
                  <Host
                    style={{ width: 44, height: 44 }}
                    colorScheme="dark"
                    ignoreSafeArea="keyboard"
                  >
                    <Menu
                      testID="attachment-approval-add"
                      label={t('media.addAttachment')}
                      systemImage="plus"
                      modifiers={[
                        buttonStyle('plain'),
                        labelStyle('iconOnly'),
                        foregroundStyle('#FFFFFF'),
                        disabled(!canAdd),
                        frame({ width: 44, height: 44 }),
                      ]}
                    >
                      <Button
                        label={t('media.photosAndVideos')}
                        systemImage="photo.on.rectangle"
                        onPress={() => props.onAdd('photo')}
                      />
                      <Button
                        label={t('media.camera')}
                        systemImage="camera"
                        onPress={() => props.onAdd('camera')}
                      />
                      <Button
                        label={t('media.file2')}
                        systemImage="doc"
                        onPress={() => props.onAdd('document')}
                      />
                    </Menu>
                  </Host>
                  <ApprovalAction
                    testID="attachment-approval-remove"
                    name="trash"
                    label={t('media.attachmentRemove', { name: selected.name })}
                    disabled={busy || selected.attempted}
                    onPress={() => props.onRemove(selected.mutationId)}
                  />
                </View>
              ) : (
                <Text numberOfLines={1} style={{ flex: 1, color: secondary, fontSize: 12 }}>
                  {t('media.attachmentRecipient', { name: recipient })}
                </Text>
              )}
              {state.phase === 'sending' ? (
                <View style={{ width: 46, height: 44, justifyContent: 'center' }}>
                  <ActivityIndicator
                    color="#FFFFFF"
                    accessibilityLabel={t('media.attachmentSending', {
                      position: 1,
                      total: state.items.length,
                    })}
                  />
                </View>
              ) : (
                <ApprovalAction
                  testID="attachment-approval-send"
                  name="arrow.up"
                  label={
                    field.retry
                      ? t('media.attachmentsRetry')
                      : t('media.attachmentSendCount', { count: field.remaining })
                  }
                  prominent
                  disabled={field.sending || !field.remaining}
                  onPress={field.onSend}
                />
              )}
            </View>
          </View>
        </View>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

/** Native presentation retains the still-focused conversation and its single draft owner. */
export function AttachmentApproval(props: AttachmentApprovalProps) {
  const { reducedMotion } = useTheme();
  return (
    <Modal
      testID={props.testID}
      visible
      presentationStyle="fullScreen"
      animationType={reducedMotion ? 'none' : 'slide'}
      supportedOrientations={['portrait', 'landscape']}
      onRequestClose={props.onClose}
    >
      <SafeAreaProvider>
        {props.libraryPicker}
        <StatusBar hidden />
        <ApprovalContent {...props} />
      </SafeAreaProvider>
    </Modal>
  );
}
