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
  type ScrollViewInstance,
} from 'react-native';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { SafeAreaProvider, SafeAreaView, useSafeAreaInsets } from 'react-native-safe-area-context';
import { Icon } from '../components/primitives';
import type { AttachmentComposerField } from './attachment-composer';
import { DraftThumbnail } from './attachment-draft-strip';
import type { AttachmentApprovalProps } from './attachment-approval.types';
import { mediaPlayback, type PlaybackOwner } from './playback-owner';
import { t } from '../i18n';
import { useTheme } from '../theme';

export const supportsMediaApproval = true;
const background = '#090A0C';
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
  name: 'xmark' | 'arrow.up' | 'pencil.tip.crop.circle' | 'trash';
  label: string;
  onPress(): void;
  disabled?: boolean;
  prominent?: boolean;
  testID: string;
}) {
  const { colors } = useTheme();
  return (
    <Host style={{ width: 48, height: 48 }} colorScheme="dark" ignoreSafeArea="keyboard">
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
          controlSize('large'),
          labelStyle('iconOnly'),
          tint(prominent ? colors.accent : '#FFFFFF'),
          foregroundStyle(prominent ? colors.onAccent : '#FFFFFF'),
          disabled(unavailable),
          frame({ width: 48, height: 48 }),
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

function ApprovalContent(props: AttachmentApprovalProps) {
  const { state, field, selectedId, recipient, busy, editing, canAdd, canEdit, error } = props;
  const selected = state.items.find((item) => item.mutationId === selectedId) ?? state.items[0];
  const { bottom } = useSafeAreaInsets();
  const [typing, setTyping] = useState(false);
  const rail = useRef<ScrollViewInstance | null>(null);
  const position = Math.max(
    0,
    state.items.findIndex((item) => item === selected),
  );
  useEffect(() => {
    rail.current?.scrollTo({ x: Math.max(0, position * 64 - 64), animated: true });
  }, [position]);
  if (!selected) return null;
  return (
    <SafeAreaView edges={['top', 'left', 'right']} style={{ flex: 1, backgroundColor: background }}>
      <KeyboardAvoidingView behavior="padding" automaticOffset style={{ flex: 1 }}>
        <View
          style={{
            flexDirection: 'row',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 12,
            paddingHorizontal: 16,
            paddingTop: 8,
            paddingBottom: 14,
          }}
        >
          <ApprovalAction
            testID="attachment-approval-close"
            name="xmark"
            label={t('common.close')}
            onPress={props.onClose}
          />
          <View
            style={{
              minWidth: 0,
              flexShrink: 1,
              maxWidth: '70%',
              alignItems: 'center',
              paddingHorizontal: 18,
              paddingVertical: 10,
              borderRadius: 26,
              backgroundColor: surface,
              gap: 2,
            }}
          >
            <Text numberOfLines={1} style={{ color: '#FFFFFF', fontSize: 14, fontWeight: '600' }}>
              {t('media.attachmentRecipient', { name: recipient })}
            </Text>
            <Text accessibilityLiveRegion="polite" style={{ color: secondary, fontSize: 11 }}>
              {t('media.attachmentCounter', { position: position + 1, total: state.items.length })}
              {field.scope.thread
                ? ` · ${t('media.attachmentInThread')}`
                : field.scope.replyTo
                  ? ` · ${t('media.attachmentInReply')}`
                  : ''}
            </Text>
          </View>
          <View style={{ width: 48 }} />
        </View>
        <View
          testID="attachment-approval-stage"
          onTouchStart={() => Keyboard.dismiss()}
          style={{
            flex: 1,
            minHeight: 48,
            marginHorizontal: 12,
            borderRadius: 28,
            borderCurve: 'continuous',
            overflow: 'hidden',
            backgroundColor: '#000000',
          }}
        >
          {!selected.uri ? (
            <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', gap: 12 }}>
              <ActivityIndicator color="#FFFFFF" />
              <Text style={{ color: secondary }}>{t('media.attachmentPreparing')}</Text>
            </View>
          ) : selected.mime.startsWith('image/') ? (
            <Image
              testID="attachment-approval-image"
              source={{ uri: selected.uri }}
              recyclingKey={selected.mutationId}
              cachePolicy="none"
              contentFit="contain"
              accessibilityLabel={selected.name}
              style={{ flex: 1 }}
            />
          ) : selected.mime.startsWith('video/') ? (
            <ApprovalVideo
              key={selected.mutationId}
              uri={selected.uri}
              active={!editing && !busy}
              isCurrent={props.isCurrent}
            />
          ) : (
            <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', gap: 12 }}>
              <Icon
                name={{ ios: 'doc', android: 'description', web: 'description' }}
                color="#FFFFFF"
                size={42}
              />
              <Text style={{ color: '#FFFFFF', paddingHorizontal: 24 }} numberOfLines={2}>
                {selected.name}
              </Text>
            </View>
          )}
        </View>
        {!typing ? (
          <View style={{ alignItems: 'center', paddingTop: 12, paddingBottom: 10 }}>
            <View
              style={{
                flexDirection: 'row',
                gap: 8,
                alignItems: 'center',
                padding: 4,
                borderRadius: 28,
                backgroundColor: surface,
              }}
            >
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
              <Host style={{ width: 48, height: 48 }} colorScheme="dark" ignoreSafeArea="keyboard">
                <Menu
                  testID="attachment-approval-add"
                  label={t('media.addAttachment')}
                  systemImage="plus"
                  modifiers={[
                    buttonStyle('plain'),
                    labelStyle('iconOnly'),
                    foregroundStyle('#FFFFFF'),
                    disabled(!canAdd),
                    frame({ width: 48, height: 48 }),
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
          </View>
        ) : null}
        {!typing ? (
          <View style={{ maxHeight: 66 }}>
            <ScrollView
              ref={rail}
              horizontal
              showsHorizontalScrollIndicator={false}
              keyboardShouldPersistTaps="handled"
              contentContainerStyle={{ gap: 8, paddingHorizontal: 16, paddingBottom: 6 }}
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
                  onPress={() => {
                    if (item !== selected) void Haptics.selectionAsync().catch(() => {});
                    props.onSelect(item.mutationId);
                  }}
                  style={{
                    width: 56,
                    height: 56,
                    padding: 2,
                    borderRadius: 14,
                    borderWidth: 2,
                    borderColor: item === selected ? '#FFFFFF' : 'transparent',
                  }}
                >
                  <View
                    style={{
                      flex: 1,
                      borderRadius: 9,
                      overflow: 'hidden',
                      backgroundColor: surface,
                    }}
                  >
                    <DraftThumbnail item={item} />
                  </View>
                </Pressable>
              ))}
            </ScrollView>
          </View>
        ) : null}
        <View
          style={{
            paddingHorizontal: 16,
            paddingTop: 8,
            paddingBottom: typing ? 8 : Math.max(bottom, 12),
            gap: 8,
          }}
        >
          {error ? (
            <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
              <Text accessibilityRole="alert" style={{ flex: 1, color: secondary, fontSize: 12 }}>
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
          <View style={{ flexDirection: 'row', alignItems: 'flex-end', gap: 10 }}>
            <View
              style={{
                flex: 1,
                paddingHorizontal: 16,
                paddingVertical: 14,
                borderRadius: 26,
                backgroundColor: surface,
              }}
            >
              <Caption key={selected.mutationId} field={field} onFocus={setTyping} />
            </View>
            {state.phase === 'sending' ? (
              <View style={{ width: 48, height: 48, justifyContent: 'center' }}>
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
        <StatusBar style="light" />
        <ApprovalContent {...props} />
      </SafeAreaProvider>
    </Modal>
  );
}
