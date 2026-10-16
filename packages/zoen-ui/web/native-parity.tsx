import { useRef, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { Messenger } from './messenger.tsx';
import type { Messages } from './messages.ts';
import { PARITY_FIXTURE, parityConfig, type ParityConfig } from './parity-config.ts';
import { NativeOnboarding } from './native-secondary.tsx';
import { BrowserSettings } from './browser-settings.tsx';
import { completeWelcome, openWelcomeInvite } from './welcome.ts';
import { nativeFixture, nativeFixtureConversations, nativeFixtureRootData } from './native-fixture-store.ts';
import { applyAppearance } from './appearance.ts';
import { configureDisplayClock } from './display-clock.ts';
import './native-parity.css';

declare const ZOEN_SOURCE_REVISION: string;
function NativeParity({ config }: { config: ParityConfig }) {
  const conversations = nativeFixtureConversations();
  const marina = nativeFixture.spaces.find(s => s.counterpart?.handle === 'marina');
  const coastal = nativeFixture.spaces.find(s => s.kind === 'community');
  const paraty = nativeFixture.spaces.find(s => s.title === 'Paraty with Marina');
  if (!marina || !coastal || !paraty) throw new Error('Native parity seed is missing required spaces.');
  const [selected, select] = useState(config.screen === 'chat' || config.screen === 'composer' ? marina.id : config.screen === 'chats' && config.viewport === 'wide' ? paraty.id : '');
  const rootData = nativeFixtureRootData();
  const destination = config.screen === 'spaces' ? 'spaces' : config.screen === 'notifications' ? 'notifications' : config.screen === 'profile' || config.screen === 'settings' ? 'profile' : 'chats';
  const [settings, setSettings] = useState(config.screen === 'settings');
  const messages = useRef<Messages | null>(null);
  return <div className={`native-parity np-${config.appearance} np-${config.viewport}`} data-screen={config.screen} data-fixture-id={config.fixture_id}>
    {config.screen === 'onboarding' ? <NativeOnboarding onContinue={() => { completeWelcome(); location.assign('/'); }} onPasteInvite={openWelcomeInvite} /> : <Messenger conversations={conversations} identity={nativeFixture.me.id} name={nativeFixture.me.name} connected={rootData.connection.state === 'online'} status="Read-only native fixture" rootData={rootData} initialDestination={destination} messages={messages} selected={selected} select={select} initialDraft={config.screen === 'composer' ? 'See you tomorrow' : ''} onSettings={() => setSettings(true)} footer={null}>
      {settings ? <BrowserSettings me={nativeFixture.me} close={() => setSettings(false)} /> : undefined}
    </Messenger>}
  </div>;
}

/** Reads actual production DOM/model witnesses; it cannot mutate app state. */
function observeNativeState(): Record<string, unknown> {
  const app = document.querySelector<HTMLElement>('.zm-app');
  const input = app?.querySelector<HTMLTextAreaElement>('.zm-main textarea') ?? null;
  const typing = app?.querySelector<HTMLElement>('.zm-live-typing') ?? null;
  const pending = JSON.parse(app?.dataset.pendingRequests ?? 'null') as { agentId: string; spaceId: string; title: string; status: string }[] | null;
  pending?.sort((a, b) => {
    const left = `${a.agentId}\u0000${a.spaceId}\u0000${a.title}\u0000${a.status}`, right = `${b.agentId}\u0000${b.spaceId}\u0000${b.title}\u0000${b.status}`;
    return left < right ? -1 : left > right ? 1 : 0;
  });
  const connection = JSON.parse(app?.dataset.connection ?? 'null') as { state: string; synced: boolean | null; pending: number | null } | null;
  return { tab: app?.dataset.rootTab ?? null, selectedSpace: app?.dataset.selectedSpace || null,
    draft: { text: input?.value ?? '', focused: input !== null && document.activeElement === input },
    notifications: { pendingCount: pending?.length ?? null, pending },
    typing: typing ? { spaceId: typing.dataset.space, fromId: typing.dataset.from, kind: 'typing' } : null,
    connection: connection ? { state: connection.state, synced: connection.synced, pending: connection.pending } : null,
    syncMode: app?.dataset.syncMode ?? null };
}

declare global { interface Window { __ZOEN_PARITY__?: { render: (value: unknown) => Promise<void>; observe: () => Record<string, unknown>; manifest: Record<string, unknown> | null }; } }
/** Opt-in DATA fixture. Every screen is the same component used by the production entry. */
export function installNativeParity(element: HTMLElement) {
  let root: Root | undefined;
  const hook = { manifest: null as Record<string, unknown> | null, observe: observeNativeState, async render(value: unknown) {
    const config = parityConfig(value);
    if ((config.viewport === 'wide') !== matchMedia('(min-width: 768px)').matches) throw new Error('Parity viewport does not match the production adaptive layout.');
    if (config.source_revision !== ZOEN_SOURCE_REVISION) throw new Error('Parity source revision differs from this web build; rebuild before capture.');
    for (const [key, expected] of Object.entries(PARITY_FIXTURE)) if (nativeFixture[key as keyof typeof nativeFixture] !== expected) throw new Error(`Native fixture ${key} is stale.`);
    document.documentElement.lang = 'en-US';
    applyAppearance(config.appearance);
    configureDisplayClock({ now: Date.parse(config.fixed_time), timeZone: config.timezone });
    root ??= createRoot(element);
    flushSync(() => root!.render(<NativeParity key={`${config.screen}-${config.appearance}-${config.viewport}`} config={config} />));
    await document.fonts.ready;
    await Promise.all(Array.from(document.images).map(image => image.decode()));
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    await Promise.allSettled(document.getAnimations().filter(animation => animation.effect?.getTiming().iterations !== Infinity).map(animation => animation.finished));
    hook.manifest = { ...config, data_source: 'native-core-export', read_only: true, screens: 'production-components', observed_state: observeNativeState(), geometry: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio } };
  } };
  window.__ZOEN_PARITY__ = hook;
}
