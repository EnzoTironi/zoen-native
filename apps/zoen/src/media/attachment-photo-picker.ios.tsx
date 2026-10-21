import { Host } from '@expo/ui';
import { BottomSheet, Button, Group, Menu, RNHostView } from '@expo/ui/swift-ui';
import {
  buttonStyle,
  controlSize,
  disabled,
  foregroundStyle,
  frame,
  interactiveDismissDisabled,
  presentationBackground,
  presentationDetents,
  presentationDragIndicator,
  labelStyle,
} from '@expo/ui/swift-ui/modifiers';
import { Image } from 'expo-image';
import { Camera, CameraView } from 'expo-camera';
import * as Haptics from 'expo-haptics';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import {
  ActivityIndicator,
  AppState,
  FlatList,
  Linking,
  PixelRatio,
  Pressable,
  Text,
  View,
  useWindowDimensions,
} from 'react-native';
import { Icon } from '../components/primitives';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { AttachmentPhotoSession, type PhotoPickerState } from './attachment-photo-session';
import { photoLibrary, type PhotoLibraryItem } from './attachment-photo-library';
import { pickAttachmentAssets } from './attachment-picker';
import { AttachmentImageSurface } from './attachment-image-surface';
import { useAttachmentKeyboardSession } from './attachment-keyboard-context';
import type { KeyboardPresentation } from './attachment-keyboard-session';
import { AttachmentPhotoKeyboard } from './attachment-photo-keyboard.ios';

function Action({
  label,
  symbol,
  onPress,
  unavailable = false,
  testID,
}: {
  label: string;
  symbol: 'xmark' | 'arrow.right' | 'chevron.left';
  onPress(): void;
  unavailable?: boolean;
  testID: string;
}) {
  return (
    <Host style={{ height: 44, width: 44 }}>
      <Button
        testID={testID}
        label={label}
        systemImage={symbol}
        onPress={onPress}
        modifiers={[
          buttonStyle('borderless'),
          controlSize('regular'),
          labelStyle('iconOnly'),
          disabled(unavailable),
          frame({ width: 44, height: 44 }),
        ]}
      />
    </Host>
  );
}

/** Two source rows share one camera cutout; subsequent rows remain virtualized. */
export function AttachmentPhotoGrid({
  state,
  session,
  onPreview,
}: {
  state: PhotoPickerState;
  session: AttachmentPhotoSession;
  onPreview(item: PhotoLibraryItem): void;
}) {
  const { width, height } = useWindowDimensions();
  const { colors } = useTheme();
  const [measuredWidth, setMeasuredWidth] = useState(width);
  const [cameraAllowed, setCameraAllowed] = useState(false);
  useEffect(() => {
    let retired = false;
    if (state.visible)
      void Camera.getCameraPermissionsAsync()
        .then((permission) => {
          if (!retired) setCameraAllowed(permission.granted);
        })
        .catch(() => {});
    return () => {
      retired = true;
    };
  }, [state.visible]);
  const columns = width > height ? 5 : 3;
  const pixel = PixelRatio.get();
  const size = Math.floor(((measuredWidth - (columns - 1)) / columns) * pixel) / pixel;
  const firstCount = (columns - 1) * 2;
  const ids = state.selected.map((item) => item.id);
  const admitted = () => session.isInteractive(state.token);
  const tile = (item: PhotoLibraryItem, x?: number, y?: number) => {
    const selected = ids.indexOf(item.id);
    return (
      <View
        key={item.id}
        style={{
          width: size,
          height: size,
          ...(x === undefined ? {} : { position: 'absolute', left: x, top: y }),
        }}
      >
        <Pressable
          testID={`attachment-photo-preview-${item.id}`}
          accessibilityRole="button"
          accessibilityLabel={t('media.previewAttachment', {
            name: item.filename ?? t('media.photosAndVideos'),
          })}
          disabled={state.busy}
          onPress={() => {
            if (admitted()) onPreview(item);
          }}
          style={{ position: 'absolute', inset: 0 }}
        >
          <Image
            source={{ uri: item.id }}
            style={{ width: size, height: size }}
            contentFit="cover"
            cachePolicy="none"
            recyclingKey={item.id}
          />
        </Pressable>
        {selected >= 0 ? (
          <View
            pointerEvents="none"
            style={{ position: 'absolute', inset: 0, borderWidth: 3, borderColor: colors.accent }}
          />
        ) : null}
        <Pressable
          testID={`attachment-photo-${item.id}`}
          accessibilityRole="checkbox"
          accessibilityLabel={item.filename ?? t('media.photosAndVideos')}
          accessibilityState={{ checked: selected >= 0, disabled: state.busy }}
          disabled={state.busy}
          onPress={() => {
            if (admitted()) {
              void Haptics.selectionAsync().catch(() => {});
              session.select(item);
            }
          }}
          style={{ position: 'absolute', right: -3, top: -3, width: 44, height: 44, padding: 10 }}
        >
          <View
            pointerEvents="none"
            style={{
              width: 24,
              height: 24,
              borderRadius: 12,
              borderWidth: selected < 0 ? 1.5 : 0,
              borderColor: '#FFFFFF',
              backgroundColor: selected >= 0 ? colors.accent : '#00000040',
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            <Text
              style={{
                color: selected >= 0 ? colors.onAccent : '#FFFFFF',
                fontSize: 14,
                fontWeight: '600',
              }}
            >
              {selected < 0 ? '' : selected + 1}
            </Text>
          </View>
        </Pressable>
        {item.mediaType === 'video' ? (
          <Text
            style={{
              position: 'absolute',
              right: 6,
              bottom: 5,
              color: '#FFFFFF',
              fontSize: 12,
              fontWeight: '600',
              textShadowColor: '#000000',
              textShadowRadius: 3,
            }}
          >{`${Math.floor((item.duration ?? 0) / 60)}:${String(Math.floor((item.duration ?? 0) % 60)).padStart(2, '0')}`}</Text>
        ) : null}
      </View>
    );
  };
  return (
    <FlatList
      onLayout={({ nativeEvent: { layout } }) => {
        if (Number.isFinite(layout.width) && layout.width > 0) setMeasuredWidth(layout.width);
      }}
      key={columns}
      testID="attachment-photos-grid"
      data={state.items.slice(firstCount)}
      keyExtractor={(item) => item.id}
      numColumns={columns}
      columnWrapperStyle={{ gap: 1 }}
      ItemSeparatorComponent={() => <View style={{ height: 1 }} />}
      style={{ flex: 1 }}
      onEndReached={() => {
        if (admitted()) void session.more();
      }}
      onEndReachedThreshold={0.6}
      initialNumToRender={18}
      maxToRenderPerBatch={18}
      windowSize={5}
      ListHeaderComponent={
        <View style={{ height: size * 2 + 2 }}>
          <Pressable
            testID="attachment-photos-camera"
            accessibilityRole="button"
            accessibilityLabel={t('media.camera')}
            disabled={state.busy}
            onPress={() => {
              if (admitted()) session.camera();
            }}
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              width: size,
              height: size * 2 + 1,
              backgroundColor: colors.surface,
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            {cameraAllowed && state.visible && !state.busy ? (
              <CameraView
                testID="attachment-photos-camera-preview"
                style={{ position: 'absolute', inset: 0 }}
                facing="back"
                active
              />
            ) : null}
            <Icon name="camera.fill" size={32} color={colors.text} />
          </Pressable>
          {state.items
            .slice(0, firstCount)
            .map((item, index) =>
              tile(
                item,
                ((index % (columns - 1)) + 1) * (size + 1),
                Math.floor(index / (columns - 1)) * (size + 1),
              ),
            )}
        </View>
      }
      renderItem={({ item }) => tile(item)}
      ListFooterComponent={
        state.loading ? (
          <ActivityIndicator style={{ padding: 20 }} color={colors.accent} />
        ) : undefined
      }
    />
  );
}

export function AttachmentPhotoPickerSheet({
  state,
  session,
  onPreferredIndex,
}: {
  state: PhotoPickerState;
  session: AttachmentPhotoSession;
  onPreferredIndex?(index: number | null): void;
}) {
  const { colors, reducedMotion } = useTheme();
  const [selection, setSelection] = useState<{ token: number; id: string | null }>({
    token: state.token,
    id: null,
  });
  const canPreview = state.visible && Boolean(state.permission?.granted);
  const matchesToken = selection.token === state.token;
  if (!matchesToken || (!canPreview && selection.id !== null))
    setSelection({ token: state.token, id: null });
  const preview =
    matchesToken && canPreview
      ? (state.items.find((item) => item.id === selection.id) ?? null)
      : null;
  const setPreviewId = (id: string | null) => setSelection({ token: state.token, id });
  const previewOwner = useRef<{ token: number; id: string | null } | null>(null);
  useLayoutEffect(() => {
    const owner = { token: state.token, id: preview?.id ?? null };
    previewOwner.current = owner;
    return () => {
      if (previewOwner.current === owner) previewOwner.current = null;
    };
  }, [state.token, preview?.id]);
  const currentPreview = () =>
    previewOwner.current?.token === state.token &&
    previewOwner.current?.id === (preview?.id ?? null);
  const approve = (item: PhotoLibraryItem | null = preview) => {
    if (!session.isInteractive(state.token) || !currentPreview()) return;
    if (item && !session.getSnapshot().selected.some((selected) => selected.id === item.id))
      session.select(item);
    if (item && !session.getSnapshot().selected.some((selected) => selected.id === item.id)) return;
    onPreferredIndex?.(
      item ? session.getSnapshot().selected.findIndex((selected) => selected.id === item.id) : null,
    );
    void session.approve();
  };
  const current = () => session.isPresented(state.token) && currentPreview();
  const allowed = state.permission?.granted;
  return (
    <Host style={{ position: 'absolute' }} pointerEvents="none">
      <BottomSheet
        isPresented={state.visible}
        onIsPresentedChange={(presented) => {
          if (!presented) session.dismissing(state.token);
        }}
        onDismiss={() => {
          void session.dismissed(state.token);
        }}
      >
        <Group
          modifiers={[
            frame({ maxWidth: Infinity, maxHeight: Infinity }),
            presentationDetents(preview ? ['large'] : ['medium', 'large']),
            presentationDragIndicator('visible'),
            presentationBackground(colors.background),
            interactiveDismissDisabled(state.busy),
          ]}
        >
          <RNHostView>
            <View
              testID="attachment-photos-picker"
              style={{ flex: 1, backgroundColor: colors.background }}
            >
              <View
                style={{
                  height: 56,
                  flexDirection: 'row',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  paddingHorizontal: 8,
                }}
              >
                <Action
                  testID="attachment-photos-close"
                  label={t('media.cancelAttachment')}
                  symbol={preview ? 'chevron.left' : 'xmark'}
                  onPress={() => {
                    if (!current()) return;
                    if (preview) setPreviewId(null);
                    else session.cancel();
                  }}
                />
                {preview ? (
                  <Text
                    numberOfLines={1}
                    style={{
                      flex: 1,
                      color: colors.text,
                      fontSize: 17,
                      fontWeight: '600',
                      textAlign: 'center',
                    }}
                  >
                    {preview.filename}
                  </Text>
                ) : (
                  <Host style={{ height: 44, flex: 1 }}>
                    <Menu
                      label={
                        state.albums.find((album) => album.id === state.albumId)?.title ??
                        t('media.photoLibraryRecents')
                      }
                      systemImage="chevron.down"
                      modifiers={[
                        buttonStyle('borderless'),
                        foregroundStyle(colors.text),
                        disabled(state.busy),
                      ]}
                    >
                      <Button
                        label={t('media.photoLibraryRecents')}
                        onPress={() => {
                          if (current()) session.album(null);
                        }}
                      />
                      {state.albums.map((album) => (
                        <Button
                          key={album.id}
                          label={album.title}
                          onPress={() => {
                            if (current()) session.album(album.id);
                          }}
                        />
                      ))}
                    </Menu>
                  </Host>
                )}
                <Action
                  testID="attachment-photos-review"
                  label={t('media.photoLibraryReview', { count: state.selected.length })}
                  symbol="arrow.right"
                  unavailable={state.busy || (!preview && !state.selected.length)}
                  onPress={() => {
                    if (current()) approve();
                  }}
                />
              </View>
              {state.permission?.accessPrivileges === 'limited' ? (
                <Pressable
                  testID="attachment-photos-limited"
                  onPress={() => {
                    if (current()) void session.manage();
                  }}
                  style={{ paddingHorizontal: 16, paddingVertical: 10 }}
                >
                  <Text style={{ color: colors.accent, fontSize: 14 }}>
                    {t('media.photoLibraryManageAccess')}
                  </Text>
                </Pressable>
              ) : null}
              {allowed ? (
                preview ? (
                  <View style={{ flex: 1 }}>
                    <AttachmentImageSurface
                      identity={preview.id}
                      uri={preview.id}
                      label={preview.filename ?? t('media.photosAndVideos')}
                      active
                      disabled={state.busy}
                      reducedMotion={reducedMotion}
                      isCurrent={() =>
                        session.isInteractive(state.token) &&
                        previewOwner.current?.token === state.token &&
                        previewOwner.current?.id === preview.id
                      }
                    />
                    <Pressable
                      testID="attachment-photo-preview-selection"
                      accessibilityRole="checkbox"
                      accessibilityLabel={preview.filename ?? t('media.photosAndVideos')}
                      accessibilityState={{
                        checked: state.selected.some((item) => item.id === preview.id),
                        disabled: state.busy,
                      }}
                      onPress={() => {
                        if (session.isInteractive(state.token) && currentPreview())
                          session.select(preview);
                      }}
                      style={{ minHeight: 44, padding: 12, alignItems: 'center' }}
                    >
                      <Text style={{ color: colors.accent, fontSize: 17 }}>
                        {state.selected.some((item) => item.id === preview.id)
                          ? `✓ ${state.selected.findIndex((item) => item.id === preview.id) + 1}`
                          : t('media.attachmentSelect', {
                              name: preview.filename ?? t('media.photosAndVideos'),
                            })}
                      </Text>
                    </Pressable>
                  </View>
                ) : (
                  <AttachmentPhotoGrid
                    state={state}
                    session={session}
                    onPreview={(item) => {
                      if (!session.isInteractive(state.token) || !currentPreview()) return;
                      if (item.mediaType === 'video') approve(item);
                      else setPreviewId(item.id);
                    }}
                  />
                )
              ) : state.loading ? (
                <ActivityIndicator style={{ flex: 1 }} />
              ) : (
                <View
                  style={{
                    flex: 1,
                    justifyContent: 'center',
                    alignItems: 'center',
                    gap: 12,
                    padding: 24,
                  }}
                >
                  <Icon name="photo.on.rectangle" size={40} color={colors.secondary} />
                  <Text style={{ color: colors.text, textAlign: 'center', fontSize: 17 }}>
                    {t('media.youCanAllowAccessInDeviceSettingsTo')}
                  </Text>
                  <Pressable
                    testID="attachment-photos-settings"
                    onPress={() => {
                      if (current()) void Linking.openSettings();
                    }}
                  >
                    <Text style={{ color: colors.accent, fontSize: 17 }}>
                      {t('media.allowPhotos')}
                    </Text>
                  </Pressable>
                </View>
              )}
              {state.error ? (
                <Pressable
                  testID="attachment-photos-retry"
                  accessibilityRole="button"
                  onPress={() => {
                    if (current()) void session.reload();
                  }}
                  style={{ padding: 12 }}
                >
                  <Text accessibilityRole="alert" style={{ color: colors.danger }}>
                    {t('media.theMediaCouldNotBeSelected')} · {t('chat.tryAgain2')}
                  </Text>
                </Pressable>
              ) : null}
              <View
                style={{
                  height: 50,
                  paddingHorizontal: 16,
                  justifyContent: 'center',
                  backgroundColor: colors.background,
                }}
              >
                <Text style={{ color: colors.text, textAlign: 'center', fontSize: 15 }}>
                  {state.busy
                    ? t('media.attachmentPreparing')
                    : t('media.photoLibraryReview', { count: state.selected.length })}
                </Text>
              </View>
            </View>
          </RNHostView>
        </Group>
      </BottomSheet>
    </Host>
  );
}
export function useAttachmentPhotoPicker() {
  const keyboard = useAttachmentKeyboardSession();
  const [keyboardRequest, setKeyboardRequest] = useState<KeyboardPresentation | null>(null);
  const preferredIndex = useRef<number | null>(null);
  const preferred = useRef<string | null>(null);
  const session = useMemo(
    () =>
      new AttachmentPhotoSession({
        ...photoLibrary,
        capture: (remaining, current) => pickAttachmentAssets('camera', remaining, current),
      }),
    [],
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useEffect(() => {
    const unsubscribe = keyboard?.subscribe(() => {
      if (keyboardRequest && !keyboard.owns(keyboardRequest.id)) session.cancel();
    });
    return () => {
      unsubscribe?.();
    };
  }, [keyboard, keyboardRequest, session]);
  useEffect(() => {
    // Registering the native observer performs a PHAsset fetch: never do it at chat mount.
    if (state.visible && state.permission?.granted)
      return photoLibrary.subscribe(session.invalidate);
  }, [session, state.visible, state.permission?.granted]);
  useEffect(() => {
    const background = AppState.addEventListener('change', (value) => {
      if (value === 'background') session.cancel();
    });
    return () => {
      background.remove();
      session.cancel();
    };
  }, [session]);
  return {
    pick: async (remaining: number, current: () => boolean) => {
      preferredIndex.current = null;
      preferred.current = null;
      const presentation = keyboard?.getSnapshot().presentation;
      const request =
        presentation?.kind === 'attachments' && keyboard?.admitPicker(presentation.id)
          ? presentation
          : null;
      if (presentation?.kind === 'attachments' && !request) return [];
      setKeyboardRequest(request);
      const files = await session.open(remaining, current);
      setKeyboardRequest(null);
      preferred.current =
        preferredIndex.current === null ? null : (files[preferredIndex.current]?.uri ?? null);
      return files;
    },
    preferredUri: () => preferred.current,
    cancel: session.cancel,
    view:
      keyboard && keyboardRequest ? (
        <AttachmentPhotoKeyboard
          state={state}
          session={session}
          keyboard={keyboard}
          request={keyboardRequest}
          onPreferredIndex={(index) => {
            preferredIndex.current = index;
          }}
        />
      ) : (
        <AttachmentPhotoPickerSheet
          state={state}
          session={session}
          onPreferredIndex={(index) => {
            preferredIndex.current = index;
          }}
        />
      ),
  };
}
