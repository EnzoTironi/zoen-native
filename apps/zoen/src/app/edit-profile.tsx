import { useEffect, useRef, useState } from 'react';
import { Stack, router } from 'expo-router';
import { Image, ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import { pickProfilePhoto } from '../profile/picker';
import { useProfilePhoto } from '../profile/use-profile-photo';
import type { PhotoDraft } from '../profile/types';
import { Avatar, Field, NativeButton } from '../components/primitives';
import { useTheme } from '../theme';

export default function EditProfile() {
  const { snapshot, store } = useClient();
  const { colors } = useTheme();
  const [account] = useState(snapshot?.account);
  const [photo, setPhoto] = useState<'keep' | 'set' | 'remove'>('keep');
  const [draft, setDraft] = useState<PhotoDraft | null>(null);
  const ownedDraft = useRef<PhotoDraft | null>(null);
  const mounted = useRef(true);
  const currentPhoto = useProfilePhoto(
    snapshot?.me?.id,
    snapshot?.me?.photo_sha256,
    snapshot?.me?.photo_ready ?? false,
  );
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      ownedDraft.current?.dispose();
    };
  }, []);
  const [name, setName] = useState(snapshot?.me?.name ?? '');
  const [handle, setHandle] = useState(snapshot?.account?.handle ?? '');
  const [bio, setBio] = useState(snapshot?.me?.bio ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{ padding: 26, gap: 24 }}
    >
      <Stack.Screen options={{ title: 'Seu perfil' }} />
      <View style={{ alignItems: 'center', gap: 16 }}>
        {(photo === 'set' ? draft?.uri : photo === 'keep' ? currentPhoto : undefined) ? (
          <Image
            accessibilityLabel="Prévia da sua foto de perfil"
            source={{ uri: photo === 'set' ? draft?.uri : currentPhoto }}
            style={{ width: 104, height: 104, borderRadius: 52 }}
          />
        ) : (
          <Avatar name={name || 'Zoen'} color={snapshot?.me?.tint_hex} size={104} />
        )}
        {store.driver.photos ? (
          <>
            <NativeButton
              label="Escolher foto"
              disabled={busy}
              onPress={() => {
                setBusy(true);
                setError(null);
                void pickProfilePhoto()
                  .then((selected) => {
                    if (!selected) return;
                    if (!mounted.current) {
                      selected.dispose();
                      return;
                    }
                    const current = store.getSnapshot().snapshot?.account;
                    if (
                      !account ||
                      current?.identity_id !== account.identity_id ||
                      current.device_id !== account.device_id
                    ) {
                      selected.dispose();
                      return;
                    }
                    ownedDraft.current?.dispose();
                    ownedDraft.current = selected;
                    setDraft(selected);
                    setPhoto('set');
                  })
                  .catch((failure: unknown) =>
                    setError(
                      failure instanceof Error ? failure.message : 'Não foi possível abrir a foto.',
                    ),
                  )
                  .finally(() => setBusy(false));
              }}
            />
            {photo === 'set' || (photo === 'keep' && snapshot?.me?.photo_sha256) ? (
              <NativeButton
                label="Remover foto"
                disabled={busy}
                onPress={() => {
                  ownedDraft.current?.dispose();
                  ownedDraft.current = null;
                  setDraft(null);
                  setPhoto('remove');
                }}
              />
            ) : null}
          </>
        ) : null}
      </View>
      <Field label="Nome" value={name} onChangeText={setName} autoComplete="name" maxLength={64} />
      <Field
        label="Seu @"
        value={handle}
        onChangeText={setHandle}
        autoCapitalize="none"
        autoCorrect={false}
        maxLength={24}
        editable={!busy}
      />
      <Field
        label="Sobre você"
        value={bio}
        onChangeText={setBio}
        multiline
        maxLength={280}
        style={{ minHeight: 120, textAlignVertical: 'top' }}
      />
      <Text style={{ color: colors.secondary, lineHeight: 22 }}>
        Seu perfil é compartilhado de forma criptografada com seus contatos.
      </Text>
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        label={busy ? 'Salvando…' : 'Salvar'}
        disabled={
          busy || !name.trim() || !/^[a-z][a-z0-9._]{2,23}$/.test(handle.trim().toLowerCase())
        }
        onPress={() => {
          setBusy(true);
          setError(null);
          void (async () => {
            const current = store.getSnapshot().snapshot?.account;
            if (
              !account ||
              current?.identity_id !== account.identity_id ||
              current.device_id !== account.device_id
            )
              throw new Error('A sessão mudou. Abra seu perfil novamente.');
            const normalized = handle.trim().toLowerCase();
            if (normalized !== store.getSnapshot().snapshot?.account?.handle)
              await store.request({ op: 'update_handle', handle: normalized });
            if (store.driver.photos) {
              await store.driver.photos.save({
                ownerId: account.identity_id,
                deviceId: account.device_id,
                name: name.trim(),
                bio: bio.trim(),
                photo,
                ...(photo === 'set' && draft ? { uri: draft.uri } : {}),
              });
              await store.refresh();
            } else
              await store.request({ op: 'update_profile', name: name.trim(), bio: bio.trim() });
          })()
            .then(() => router.back())
            .catch((failure: unknown) =>
              setError(
                failure instanceof Error ? failure.message : 'Não foi possível salvar seu perfil.',
              ),
            )
            .finally(() => setBusy(false));
        }}
      />
      <NativeButton label="Cancelar" disabled={busy} onPress={() => router.back()} />
    </ScrollView>
  );
}
