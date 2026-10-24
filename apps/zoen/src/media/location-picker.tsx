import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Text, View } from 'react-native';
import { randomUUID } from 'expo-crypto';
import { Field, NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { stageLocationAttachment } from './attachment-route-actions';
import { chooseStaticPoint, prepareLocationFile } from './attachment-route-platform';
import { sensitiveCacheWork } from './cache-work';
import { LocationMap } from './location-map';
import { LocationSelection } from './location-selection';
import type { MediaDraftChannel } from './media-draft-channel';
import type { MediaDriver } from './types';

export function LocationPicker({
  media,
  channel,
  current,
  replyTo,
  onStaged,
}: {
  media: MediaDriver;
  channel: MediaDraftChannel | null;
  current(): boolean;
  replyTo(): string | null;
  onStaged(): void;
}) {
  const { colors } = useTheme();
  // The route keys the destination and this session retains its original channel/driver admission.
  const [session] = useState(
    () => new LocationSelection({ media, current, gps: chooseStaticPoint }),
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const [query, setQuery] = useState('');
  const [caption, setCaption] = useState('');
  const [held, setHeld] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const working = useRef(false);
  const attempted = useRef(false);
  useEffect(() => {
    // The parent admits this keyed lifetime after its own focus/foreground
    // transition. A child focus effect can otherwise run before that admission.
    session.activate(randomUUID());
    return () => session.retire();
  }, [session]);
  const binding = state.binding;
  const valid = () => session.current(binding) && current();
  const selectable = () => valid() && !attempted.current && !working.current;
  const locked = !valid() || held || busy;
  if (!media.searchLocations || !media.readLocation)
    return <Text style={{ color: colors.secondary }}>{t('attachmentTabs.mapUnavailable')}</Text>;
  return (
    <View style={{ gap: 12 }}>
      {binding ? (
        <LocationMap
          binding={binding}
          editable={!locked}
          point={state.point ?? undefined}
          label={t('attachmentTabs.locationPreview')}
          onPointChange={(point) => {
            if (selectable()) session.select(binding, point);
          }}
        />
      ) : null}
      <Field
        label={t('attachmentTabs.searchLocation')}
        value={query}
        placeholder={t('attachmentTabs.searchLocation')}
        editable={!locked}
        onChangeText={(value) => {
          if (selectable()) setQuery(value);
        }}
      />
      <NativeButton
        label={t('attachmentTabs.searchLocation')}
        secondary
        disabled={locked || state.busy || !query.trim()}
        onPress={() => {
          if (selectable() && binding) void session.search(binding, query);
        }}
      />
      <NativeButton
        label={t('attachmentTabs.chooseLocation')}
        secondary
        disabled={locked || state.busy}
        onPress={() => {
          if (selectable() && binding) void session.gps(binding);
        }}
      />
      {state.results.map((point, index) => (
        <NativeButton
          key={index}
          secondary
          label={point.label ?? `${point.latitude}, ${point.longitude}`}
          disabled={locked}
          onPress={() => {
            if (selectable() && binding) session.select(binding, point);
          }}
        />
      ))}
      {state.point ? (
        <>
          <Text style={{ color: colors.text }}>
            {state.point.label ?? `${state.point.latitude}, ${state.point.longitude}`}
          </Text>
          <Field
            label={t('attachmentTabs.locationCaption')}
            value={caption}
            placeholder={t('attachmentTabs.locationCaption')}
            editable={!locked}
            onChangeText={(value) => {
              if (selectable()) setCaption(value);
            }}
          />
        </>
      ) : null}
      {state.error || failed ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {t('attachmentTabs.failed')}
        </Text>
      ) : null}
      {held ? (
        <Text style={{ color: colors.secondary }}>{t('attachmentTabs.contactPending')}</Text>
      ) : null}
      <NativeButton
        label={t(held ? 'attachmentTabs.retryContact' : 'attachmentTabs.addToMessage')}
        disabled={!valid() || busy || !channel || (!state.point && !held)}
        onPress={() => {
          if (!valid() || working.current || !channel || (!state.point && !attempted.current))
            return;
          working.current = true;
          setBusy(true);
          setFailed(false);
          void sensitiveCacheWork
            .run(async () => {
              if (attempted.current) await channel.flush();
              else if (state.point)
                await stageLocationAttachment({
                  media: channel,
                  point: state.point,
                  caption,
                  current: valid,
                  replyTo,
                  prepare: prepareLocationFile,
                  nextId: randomUUID,
                  admitted: () => {
                    attempted.current = true;
                    setHeld(true);
                  },
                });
              if (valid()) onStaged();
            })
            .catch(() => {
              if (valid()) setFailed(true);
            })
            .finally(() => {
              working.current = false;
              setBusy(false);
            });
        }}
      />
    </View>
  );
}
