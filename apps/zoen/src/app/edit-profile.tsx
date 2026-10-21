import { t } from '../i18n';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Stack, router, useFocusEffect } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Image } from 'expo-image';
import { useClient } from '../core/provider';
import { pickProfilePhoto } from '../profile/picker';
import { useProfilePhoto } from '../profile/use-profile-photo';
import type { PhotoDraft } from '../profile/types';
import { Avatar, Field } from '../components/primitives';
import { ProfileSection } from '../profile/profile-section';
import { peerInfo } from '../profile/telegram-profile-presentation';
import { useTheme } from '../theme';

export default function EditProfile() {
  const { snapshot, store } = useClient();
  const { presentation: p } = useTheme();
  const insets = useSafeAreaInsets();
  const [account] = useState(snapshot?.account);
  const [ownerEpoch] = useState(store.sessionEpoch);
  const [photo, setPhoto] = useState<'keep' | 'set' | 'remove'>('keep');
  const [draft, setDraft] = useState<PhotoDraft | null>(null);
  const ownedDraft = useRef<PhotoDraft | null>(null);
  const pendingSave = useRef<Promise<void> | null>(null);
  const working = useRef(false);
  const mounted = useRef(true);
  const active = useRef(false);
  useFocusEffect(
    useCallback(() => {
      active.current = true;
      return () => {
        active.current = false;
      };
    }, []),
  );
  const currentPhoto = useProfilePhoto(
    snapshot?.me?.id,
    snapshot?.me?.photo_sha256,
    snapshot?.me?.photo_ready ?? false,
  );
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      const owned = ownedDraft.current;
      ownedDraft.current = null;
      // The native save may still be reading this exact draft URI after a back gesture.
      // Retain it through that save's settlement; a late completion cannot navigate another route.
      if (owned && pendingSave.current)
        void pendingSave.current.finally(() => owned.dispose()).catch(() => {});
      else owned?.dispose();
    };
  }, []);
  const [name, setName] = useState(snapshot?.me?.name ?? '');
  const [handle, setHandle] = useState(snapshot?.account?.handle ?? '');
  const [bio, setBio] = useState(snapshot?.me?.bio ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ready = Boolean(name.trim() && /^[a-z][a-z0-9._]{2,23}$/.test(handle.trim().toLowerCase()));
  const sameOwner = () => {
    const current = store.getSnapshot().snapshot?.account;
    return Boolean(
      account &&
      ownerEpoch === store.sessionEpoch &&
      current?.identity_id === account.identity_id &&
      current.device_id === account.device_id,
    );
  };
  async function choosePhoto() {
    if (working.current || !mounted.current || !active.current || !sameOwner()) return;
    working.current = true;
    setBusy(true);
    setError(null);
    try {
      const selected = await pickProfilePhoto();
      if (!selected) return;
      if (!mounted.current || !active.current || !sameOwner()) {
        selected.dispose();
        return;
      }
      ownedDraft.current?.dispose();
      ownedDraft.current = selected;
      setDraft(selected);
      setPhoto('set');
    } catch (failure) {
      if (mounted.current)
        setError(failure instanceof Error ? failure.message : t('profile.openPhotoFailed'));
    } finally {
      working.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  function removePhoto() {
    if (working.current || !mounted.current || !active.current || !sameOwner()) return;
    ownedDraft.current?.dispose();
    ownedDraft.current = null;
    setDraft(null);
    setPhoto('remove');
  }
  function save() {
    if (working.current || !mounted.current || !active.current || !ready) return;
    working.current = true;
    setBusy(true);
    setError(null);
    const captured = {
      name: name.trim(),
      bio: bio.trim(),
      handle: handle.trim().toLowerCase(),
      photo,
      uri: draft?.uri,
    };
    const task = Promise.resolve().then(async () => {
      if (!account || !sameOwner()) throw new Error(t('profile.sessionChanged'));
      if (captured.handle !== store.getSnapshot().snapshot?.account?.handle)
        await store.request({ op: 'update_handle', handle: captured.handle });
      if (!sameOwner()) throw new Error(t('profile.sessionChanged'));
      if (store.driver.photos) {
        await store.driver.photos.save({
          ownerId: account.identity_id,
          deviceId: account.device_id,
          name: captured.name,
          bio: captured.bio,
          photo: captured.photo,
          ...(captured.photo === 'set' && captured.uri ? { uri: captured.uri } : {}),
        });
        if (sameOwner()) await store.refresh();
      } else await store.request({ op: 'update_profile', name: captured.name, bio: captured.bio });
    });
    pendingSave.current = task;
    void task
      .then(
        () => {
          if (mounted.current && active.current && sameOwner()) router.back();
        },
        (failure: unknown) => {
          if (mounted.current)
            setError(failure instanceof Error ? failure.message : t('profile.saveFailed'));
        },
      )
      .finally(() => {
        if (pendingSave.current === task) pendingSave.current = null;
        working.current = false;
        if (mounted.current) setBusy(false);
      });
  }
  const shownPhoto = photo === 'set' ? draft?.uri : photo === 'keep' ? currentPhoto : undefined;
  return (
    <ScrollView
      testID="edit-profile-screen"
      contentInsetAdjustmentBehavior="never"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        paddingHorizontal: 16,
        paddingTop: insets.top + peerInfo.avatarTop,
        paddingBottom: Math.max(insets.bottom, 16) + 24,
        gap: 24,
      }}
    >
      <Stack.Screen
        options={{
          title: '',
          headerTransparent: true,
          headerShadowVisible: false,
          headerLeft: () => (
            <Pressable
              testID="edit-profile-cancel"
              accessibilityRole="button"
              accessibilityLabel={t('access.cancel')}
              disabled={busy}
              onPress={() => router.back()}
              style={{
                minHeight: 44,
                justifyContent: 'center',
                paddingRight: 12,
                opacity: busy ? 0.4 : 1,
              }}
            >
              <Text style={{ color: p.navigation.control, fontSize: 17 }}>
                {t('access.cancel')}
              </Text>
            </Pressable>
          ),
          headerRight: () => (
            <Pressable
              testID="edit-profile-save"
              accessibilityRole="button"
              accessibilityLabel={busy ? t('profile.saving') : t('profile.save')}
              accessibilityState={{ disabled: busy || !ready, busy }}
              disabled={busy || !ready}
              onPress={save}
              style={{
                minHeight: 44,
                minWidth: 44,
                justifyContent: 'center',
                alignItems: 'flex-end',
                opacity: busy || !ready ? 0.4 : 1,
              }}
            >
              <Text style={{ color: p.navigation.control, fontSize: 17, fontWeight: '600' }}>
                {t('profile.save')}
              </Text>
            </Pressable>
          ),
        }}
      />
      <View style={{ alignItems: 'center' }}>
        {shownPhoto ? (
          <Image
            accessibilityLabel={t('profile.preview')}
            cachePolicy="none"
            source={{ uri: shownPhoto }}
            style={{
              width: peerInfo.avatar,
              height: peerInfo.avatar,
              borderRadius: peerInfo.avatar / 2,
            }}
          />
        ) : (
          <Avatar
            name={name || t('profile.name')}
            color={snapshot?.me?.tint_hex}
            size={peerInfo.avatar}
          />
        )}
        {store.driver.photos ? (
          <>
            <Pressable
              testID="edit-profile-photo"
              accessibilityRole="button"
              accessibilityLabel={t('profile.choosePhoto')}
              disabled={busy}
              onPress={() => {
                void choosePhoto();
              }}
              style={{
                minHeight: 44,
                marginTop: -1,
                justifyContent: 'center',
                opacity: busy ? 0.4 : 1,
              }}
            >
              <Text style={{ color: p.list.accent, fontSize: 17 }}>{t('profile.choosePhoto')}</Text>
            </Pressable>
            {photo === 'set' || (photo === 'keep' && snapshot?.me?.photo_sha256) ? (
              <Pressable
                testID="edit-profile-remove-photo"
                accessibilityRole="button"
                disabled={busy}
                onPress={removePhoto}
                style={{ minHeight: 44, justifyContent: 'center', opacity: busy ? 0.4 : 1 }}
              >
                <Text style={{ color: p.destructive, fontSize: 17 }}>
                  {t('profile.removePhoto')}
                </Text>
              </Pressable>
            ) : null}
          </>
        ) : null}
      </View>
      <View style={{ gap: 12 }}>
        <Field
          label={t('profile.name')}
          value={name}
          onChangeText={setName}
          autoComplete="name"
          maxLength={64}
          editable={!busy}
        />
        <Field
          label={t('profile.handle')}
          value={handle}
          onChangeText={setHandle}
          autoCapitalize="none"
          autoCorrect={false}
          maxLength={24}
          editable={!busy}
        />
      </View>
      <View style={{ gap: 8 }}>
        <Field
          label={t('profile.about')}
          value={bio}
          onChangeText={setBio}
          multiline
          maxLength={280}
          editable={!busy}
          style={{ minHeight: 88, textAlignVertical: 'top', lineHeight: 22 }}
        />
        <Text
          style={{ color: p.list.secondary, fontSize: 13, lineHeight: 18, paddingHorizontal: 16 }}
        >
          {t('profile.encryptedNotice')}
        </Text>
      </View>
      {error ? (
        <ProfileSection>
          <Text
            accessibilityRole="alert"
            selectable
            style={{ padding: 16, color: p.destructive, fontSize: 15 }}
          >
            {error}
          </Text>
        </ProfileSection>
      ) : null}
    </ScrollView>
  );
}
