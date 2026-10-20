import type { ClientStore } from '../core/store';
import type { CoreDriver, ItemDetail } from '../core/types';
import type { AppResourceDto } from '../core/generated/AppResourceDto';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import { t } from '../i18n/index.ts';
import type { ConsentChoice } from './device-consent-queue';
import { authorizedNetworkHosts } from './network.ts';
import { readVerifiedMiniappResource } from './resource-load.ts';
import type { MiniappManifest } from './sandbox';

export type NetworkConsentView = Readonly<{
  itemTitle: string;
  spaceTitle: string;
  manifest: MiniappManifest;
  allowedHosts: readonly string[];
}>;

type Ports = {
  store: Pick<ClientStore, 'sessionEpoch' | 'getSnapshot'>;
  read: CoreDriver['request'];
  write: CoreDriver['request'];
  ask(capability: string, purpose: string): Promise<ConsentChoice>;
  cancelConsent(): void;
};

/** Fresh permission-route authority. It never borrows a blurred frame's callback. */
export function createNetworkConsent(itemId: string, ports: Ports) {
  const initial = ports.store.getSnapshot();
  const epoch = ports.store.sessionEpoch;
  const owner = (state: ReturnType<ClientStore['getSnapshot']>) =>
    JSON.stringify([
      state.mode,
      state.snapshot?.account?.identity_id ?? state.snapshot?.me?.id ?? '',
      state.snapshot?.account?.device_id ?? '',
    ]);
  const expected = owner(initial);
  let active = true;
  let binding: { item: ItemDetail; spec: AppSpecDto; raw: Readonly<AppResourceDto> } | null = null;
  let loaded: NetworkConsentView | null = null;
  let readFlight: Promise<NetworkConsentView | null> | null = null;
  let grantFlight: Promise<void> | null = null;
  const closed = () => new Error(t('miniapps.thisInterfaceHasClosedOrTheAccountHas'));
  function check() {
    const state = ports.store.getSnapshot();
    const account = state.snapshot?.account;
    const currentBinding = binding;
    if (
      !active ||
      epoch !== ports.store.sessionEpoch ||
      state.phase !== 'ready' ||
      state.erasePending ||
      owner(state) !== expected ||
      (state.mode === 'account' && (!account?.unlocked || account.setup_pending)) ||
      (currentBinding &&
        !state.snapshot?.conversations.some((chat) => chat.id === currentBinding.item.space_id))
    )
      throw closed();
  }
  async function itemAndSpec() {
    check();
    const item = await ports.read({ op: 'item', item_id: itemId });
    check();
    if (item.id !== itemId || !item.app) throw closed();
    const specs = await ports.read({ op: 'app_specs' });
    check();
    const spec = specs.find((value) => value.id === item.app?.app_id);
    if (!spec) throw new Error(t('miniapps.theMiniappCouldNotBeOpened'));
    return { item, spec };
  }
  async function load() {
    const { item, spec } = await itemAndSpec();
    if (!spec.has_view) return null;
    let raw: Readonly<AppResourceDto> | null = null;
    const verified = await readVerifiedMiniappResource(
      async () => {
        raw = Object.freeze({
          ...(await ports.read({ op: 'app_resource', uri: spec.resource_uri })),
        });
        return raw;
      },
      spec.id,
      spec.resource_uri,
      check,
    );
    check();
    if (!raw) throw closed();
    binding = { item, spec, raw };
    check();
    const allowedHosts = await authorizedNetworkHosts(
      verified.manifest.allowedDomains,
      (capability) => ports.read({ op: 'app_device_allowed', item_id: itemId, capability }),
      check,
    );
    check();
    loaded = Object.freeze({
      itemTitle: item.title,
      spaceTitle: item.space_title,
      manifest: verified.manifest,
      allowedHosts: Object.freeze(allowedHosts),
    });
    return loaded;
  }
  async function revalidate() {
    const current = await itemAndSpec();
    if (
      !binding ||
      current.item.space_id !== binding.item.space_id ||
      current.item.app?.app_id !== binding.item.app?.app_id ||
      current.spec.resource_uri !== binding.spec.resource_uri ||
      !current.spec.has_view
    )
      throw closed();
    const fresh = await ports.read({ op: 'app_resource', uri: binding.spec.resource_uri });
    check();
    // Equality to the previously verified immutable bytes/policy needs no second 8MiB digest.
    for (const key of ['uri', 'mime_type', 'text', 'manifest_json', 'prefers_border'] as const)
      if (fresh[key] !== binding.raw[key]) throw closed();
  }
  async function allow() {
    const policy = await (readFlight ??= load());
    check();
    if (!policy) return;
    for (const host of new Set(policy.manifest.allowedDomains)) {
      const capability = `net:${host}`;
      const allowed = await ports.read({ op: 'app_device_allowed', item_id: itemId, capability });
      check();
      if (allowed) continue;
      const purpose = policy.manifest.networkPurpose || t('miniapps.loadContentFromThisDomain');
      const choice = await ports.ask(capability, purpose);
      check();
      if (choice === 'deny') throw new Error(t('miniapps.youDidNotAllowThisAccess'));
      await revalidate();
      check();
      const nowAllowed = await ports.read({
        op: 'app_device_allowed',
        item_id: itemId,
        capability,
      });
      check();
      if (nowAllowed) continue;
      await ports.write({
        op: 'grant_app_device',
        item_id: itemId,
        capability,
        purpose,
        always: choice === 'always',
      });
      check();
    }
  }
  return {
    check,
    async read() {
      check();
      const policy = await (readFlight ??= load());
      check();
      return policy;
    },
    async grant() {
      check();
      if (!grantFlight) {
        grantFlight = allow().finally(() => {
          grantFlight = null;
        });
      }
      return grantFlight;
    },
    close() {
      if (!active) return;
      active = false;
      ports.cancelConsent();
    },
  };
}
