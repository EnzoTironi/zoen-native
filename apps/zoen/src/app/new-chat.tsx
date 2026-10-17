import { t, formatLocale } from '../i18n/index.ts';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useEffect, useMemo, useState } from 'react';
import { router } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import type { Persona } from '../core/generated/Persona';
import { useTheme } from '../theme';
import { Field, NativeButton } from '../components/primitives';

export default function NewChat() {
  const { store, snapshot } = useClient();
  const { colors } = useTheme();
  const [query, setQuery] = useState('');
  const [people, setPeople] = useState<Persona[]>([]);
  const [title, setTitle] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [kind, setKind] = useState<'direct' | 'group' | 'community'>('direct');
  const [error, setError] = useState<string | null>(null);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const local = useMemo(
    () => [
      ...new Map(
        snapshot?.conversations
          .flatMap((chat) => chat.members)
          .filter((person) => !person.is_me)
          .map((person) => [person.id, person]) ?? [],
      ).values(),
    ],
    [snapshot],
  );
  const matches = [
    ...new Map(
      [...local, ...people]
        .filter((person) =>
          `${person.name} ${person.handle}`
            .toLocaleLowerCase(formatLocale)
            .includes(query.toLocaleLowerCase(formatLocale)),
        )
        .map((person) => [person.id, person]),
    ).values(),
  ];
  useEffect(() => {
    let active = true;
    const timer = setTimeout(() => {
      if (query.trim())
        void store.driver
          .request({ op: 'find_people', query })
          .then((result) => {
            if (active) {
              setPeople(result);
              setLookupError(null);
            }
          })
          .catch((failure: unknown) => {
            if (active) {
              setPeople([]);
              setLookupError(
                failure instanceof Error
                  ? failure.message
                  : t('chat.peopleCouldNotBeFoundOnTheServer'),
              );
            }
          });
      else {
        setPeople([]);
        setLookupError(null);
      }
    }, 250);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [query, store]);
  function opened(id: string) {
    router.dismiss();
    router.push({ pathname: '/chat/[id]', params: { id } });
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{ padding: 24, gap: 22, backgroundColor: 'transparent' }}
    >
      <View style={{ flexDirection: 'row', gap: 8, flexWrap: 'wrap' }}>
        {(
          [
            { value: 'direct', title: t('chat.person') },
            { value: 'group', title: t('chat.group') },
            { value: 'community', title: t('chat.community') },
          ] as const
        ).map((choice) => (
          <NativeButton
            key={choice.value}
            label={choice.title}
            secondary={kind !== choice.value}
            disabled={busy}
            onPress={() => setKind(choice.value)}
          />
        ))}
      </View>
      {kind !== 'direct' ? (
        <Field
          label={kind === 'group' ? t('chat.groupName') : t('chat.communityName')}
          value={title}
          onChangeText={setTitle}
        />
      ) : null}
      {kind !== 'community' ? (
        <Field
          label={kind === 'group' ? t('chat.addPeople') : t('chat.searchPeople')}
          placeholder={t('chat.nameOr')}
          value={query}
          onChangeText={setQuery}
          autoCapitalize="none"
          autoCorrect={false}
        />
      ) : null}
      {lookupError && kind !== 'community' ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {lookupError}
        </Text>
      ) : null}
      {kind !== 'community'
        ? matches.map((person) => (
            <Pressable
              disabled={busy}
              key={person.id}
              onPress={() => {
                if (kind === 'group')
                  setSelected((ids) =>
                    ids.includes(person.id)
                      ? ids.filter((id) => id !== person.id)
                      : [...ids, person.id],
                  );
                else {
                  setBusy(true);
                  setError(null);
                  void store
                    .request({ op: 'start_direct', identity_id: person.id })
                    .then(opened)
                    .catch((failure: unknown) =>
                      setError(
                        failure instanceof Error
                          ? failure.message
                          : t('chat.theChatCouldNotBeStarted'),
                      ),
                    )
                    .finally(() => setBusy(false));
                }
              }}
              style={{ flexDirection: 'row', alignItems: 'center', gap: 14, paddingVertical: 8 }}
              accessibilityRole={kind === 'group' ? 'checkbox' : 'button'}
              aria-checked={kind === 'group' ? selected.includes(person.id) : undefined}
            >
              <ProfileAvatar person={person} name={person.name} color={person.tint_hex} />
              <View style={{ flex: 1 }}>
                <Text style={{ fontSize: 17, fontWeight: '600', color: colors.text }}>
                  {person.name}
                </Text>
                <Text style={{ color: colors.secondary, marginTop: 4 }}>@{person.handle}</Text>
              </View>
              {kind === 'group' && selected.includes(person.id) ? (
                <Text style={{ color: colors.accent }}>✓</Text>
              ) : null}
            </Pressable>
          ))
        : null}
      {kind === 'group' ? (
        <Text style={{ color: colors.secondary, fontSize: 13 }}>
          {t('chat.selectedPeople', { count: selected.length })}
        </Text>
      ) : null}
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      {kind !== 'direct' ? (
        <NativeButton
          label={
            busy
              ? t('chat.creating')
              : kind === 'group'
                ? t('chat.createGroup')
                : t('chat.createCommunity')
          }
          disabled={!title.trim() || busy}
          onPress={() => {
            setBusy(true);
            setError(null);
            void store
              .request(
                kind === 'group'
                  ? { op: 'create_group', title: title.trim(), member_ids: selected }
                  : { op: 'create_community', title: title.trim() },
              )
              .then(opened)
              .catch((failure: unknown) =>
                setError(
                  failure instanceof Error ? failure.message : t('chat.theChatCouldNotBeCreated'),
                ),
              )
              .finally(() => setBusy(false));
          }}
        />
      ) : null}
    </ScrollView>
  );
}
