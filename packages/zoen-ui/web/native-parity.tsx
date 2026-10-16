import { useRef, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { Messenger } from './messenger.tsx';
import type { Messages } from './messages.ts';
import { PARITY_FIXTURE, parityConfig, type ParityConfig } from './parity-config.ts';
import { NativeOnboarding } from './native-secondary.tsx';
import { BrowserSettings } from './browser-settings.tsx';
import { completeWelcome, openWelcomeInvite } from './welcome.ts';
import { nativeFixture, nativeFixtureConversations } from './native-fixture-store.ts';
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
  const [selected, select] = useState(config.screen === 'spaces' ? coastal.id : config.screen === 'chats' ? config.viewport === 'wide' ? paraty.id : '' : marina.id);
  const [settings, setSettings] = useState(config.screen === 'settings');
  const messages = useRef<Messages | null>(null);
  return <div className={`native-parity np-${config.appearance} np-${config.viewport}`} data-screen={config.screen} data-fixture-id={config.fixture_id}>
    {config.screen === 'onboarding' ? <NativeOnboarding onContinue={() => { completeWelcome(); location.assign('/'); }} onPasteInvite={openWelcomeInvite} /> : <Messenger conversations={conversations} identity={nativeFixture.me.id} name={nativeFixture.me.name} connected status="Read-only native fixture" messages={messages} selected={selected} select={select} initialDraft={config.screen === 'composer' ? 'See you tomorrow' : ''} onSettings={() => setSettings(true)} footer={null}>
      {settings ? <BrowserSettings me={nativeFixture.me} close={() => setSettings(false)} /> : undefined}
    </Messenger>}
  </div>;
}

declare global { interface Window { __ZOEN_PARITY__?: { render: (value: unknown) => Promise<void>; manifest: Record<string, unknown> | null }; } }
/** Opt-in DATA fixture. Every screen is the same component used by the production entry. */
export function installNativeParity(element: HTMLElement) {
  let root: Root | undefined;
  const hook = { manifest: null as Record<string, unknown> | null, async render(value: unknown) {
    const config = parityConfig(value);
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
    hook.manifest = { ...config, data_source: 'native-core-export', read_only: true, screens: 'production-components' };
  } };
  window.__ZOEN_PARITY__ = hook;
}
