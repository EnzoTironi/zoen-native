import { t } from '../i18n/index.ts';
import type { CoreDriver } from '../core/types';
import type { VerifiedMiniappResource } from './sandbox';
import { createNativeCanonicalBinaryHost } from './canonical-binary-native.ts';
import { canonicalBinaryTools } from './canonical-binary.ts';
import { MiniappSession } from './protocol.ts';
import { frameAuthority } from './lifetime.ts';
import type { MiniappDocumentController } from './document-controller';
import type { DeckcraftExportFormat, DeckcraftExportHead } from './deckcraft-export';
import { deckcraftFeedbackReason, type DeckcraftFeedbackReason } from './deckcraft-feedback.ts';

type Phase = 'saved' | 'saving' | 'pending' | 'error';
type Options = {
  itemId: string;
  title: () => string;
  verified: VerifiedMiniappResource;
  driver: Pick<CoreDriver, 'request'>;
  mutate: CoreDriver['request'];
  check: () => void;
  id: () => string;
  context: () => Record<string, unknown>;
  send: (message: unknown) => void;
  disposeFrame: () => void;
  close: () => void;
  share: (
    format: DeckcraftExportFormat,
    head: DeckcraftExportHead,
    call: (name: string, args: Record<string, unknown>) => Promise<unknown>,
  ) => Promise<void>;
  onState: (state: {
    phase: Phase;
    opened: boolean;
    blocked: boolean;
    error: string | null;
    reason: DeckcraftFeedbackReason | null;
  }) => void;
};

/** One focused document owns its official protocol, bounded transfers and final
 * save. React owns presentation only; the Rust core remains the sole journal. */
export function createDeckcraftSession(options: Options) {
  const lifetime = frameAuthority(options.check);
  const binary = createNativeCanonicalBinaryHost({
    itemId: options.itemId,
    driver: options.driver,
    mutate: options.mutate,
    check: lifetime.check,
    id: options.id,
  });
  let live = true;
  let opened = false;
  let retiring = false;
  let recoveryRequired = false;
  let queued = 0;
  let phase: Phase = 'saved';
  let error: string | null = null;
  let reason: DeckcraftFeedbackReason | null = null;
  let tail = Promise.resolve();
  let closing: Promise<void> | null = null;
  const publish = () => {
    if (live)
      options.onState({
        phase,
        opened,
        blocked: !opened || retiring || queued > 0 || recoveryRequired,
        error,
        reason,
      });
  };
  const current = new MiniappSession({
    send: options.send,
    resourceUri: options.verified.resource.uri,
    tools: new Set([...canonicalBinaryTools, 'app_binary_share']),
    initialize: () => ({
      protocolVersion: '2026-01-26',
      hostInfo: { name: 'zoen', version: '0.1.0' },
      hostCapabilities: {
        serverTools: {},
        serverResources: {},
        updateModelContext: { structuredContent: {} },
        sandbox: { csp: { connectDomains: [] }, permissions: {} },
      },
      hostContext: options.context(),
    }),
    context: options.context,
    state: () => ({
      content: [{ type: 'text', text: options.title() }],
      structuredContent: { checkpointId: options.itemId },
      isError: false,
    }),
    resource: async () => {
      lifetime.check();
      const resource = options.verified.resource;
      return {
        contents: [{ uri: resource.uri, mimeType: resource.mime_type, text: resource.text }],
      };
    },
    call: async (name, args) => {
      lifetime.check();
      if (name === 'app_binary_share') {
        if (
          args.id !== options.itemId ||
          !['pdf', 'pptx'].includes(String(args.format)) ||
          Object.keys(args).some((key) => !['id', 'format'].includes(key))
        )
          throw Error(t('decks.exportFailed'));
        await share(args.format as DeckcraftExportFormat);
        lifetime.check();
        return { content: [], structuredContent: { presented: true } };
      }
      const result = await binary.call(name, args);
      lifetime.check();
      return result;
    },
    updateModelContext: async (params) => {
      lifetime.check();
      const status = (
        params.structuredContent as
          | {
              zoen?: {
                binarySave?: {
                  itemId?: unknown;
                  phase?: unknown;
                  ready?: unknown;
                  reason?: unknown;
                };
              };
            }
          | undefined
      )?.zoen?.binarySave;
      if (
        !status ||
        status.itemId !== options.itemId ||
        !['saved', 'saving', 'pending', 'error'].includes(String(status.phase)) ||
        typeof status.ready !== 'boolean'
      )
        return {};
      phase = status.phase as Phase;
      reason = phase === 'error' ? deckcraftFeedbackReason(status.reason) : null;
      // Once an editable accepted Session was exposed, no later display label
      // can remove its obligation to flush before ordinary dismissal.
      opened ||= status.ready && phase === 'saved';
      // A timed-out teardown can still complete inside the guest. Its status
      // notification proves neither that input resumed nor that Close is safe.
      if (!recoveryRequired)
        error =
          phase === 'error'
            ? opened
              ? reason === 'autosave_busy'
                ? t('decks.editInProgress')
                : reason === 'layout_failed' || reason === 'host_ui_failed'
                  ? t('decks.viewFailed')
                  : reason === 'export_failed'
                    ? t('decks.exportFailed')
                    : t('decks.saveFailed')
              : status.reason === 'graphics_unavailable'
                ? t('decks.graphicsUnavailable')
                : status.reason === 'canvas_missing'
                  ? t('decks.canvasUnavailable')
                  : status.reason === 'resource_fetch_failed'
                    ? t('decks.resourceUnavailable')
                    : status.reason === 'module_init_failed'
                      ? t('decks.moduleUnavailable')
                      : status.reason === 'document_open_failed'
                        ? t('decks.documentUnavailable')
                        : status.reason === 'runtime_start_failed'
                          ? t('decks.runtimeUnavailable')
                          : t('decks.unavailable')
            : null;
      publish();
      return {};
    },
    openLink: async () => {
      throw new Error(t('miniapps.thisFeatureIsNotAvailableOnThisHost'));
    },
    share: async () => {
      throw new Error(t('miniapps.thisFeatureIsNotAvailableOnThisHost'));
    },
    native: async () => {
      throw new Error(t('miniapps.thisFeatureIsNotAvailableOnThisHost'));
    },
    haptic: () => {},
    resize: () => {},
    close: options.close,
  });
  const controller: MiniappDocumentController = {
    flush: (intent) => {
      try {
        lifetime.check();
      } catch (error) {
        return Promise.reject(error);
      }
      const retire = !!intent?.retire;
      if (retiring) return closing ?? tail;
      if (retire) retiring = true;
      // The cover has admitted no manual input until the first canonical open.
      // Closing an unfinished download need not wait for an editable Session.
      if (!opened) {
        publish();
        return Promise.resolve();
      }
      queued++;
      publish();
      const operation = tail
        .catch(() => {})
        .then(async () => {
          lifetime.check();
          await current.flushResource();
          lifetime.check();
          if (!retire && !retiring) {
            const resumed = await current.callAppTool('resume_document');
            lifetime.check();
            if (resumed.isError) throw new Error(t('decks.saveFailed'));
          }
          phase = 'saved';
          if (retire || !retiring) {
            recoveryRequired = false;
            error = null;
            reason = null;
          }
        })
        .catch((reason: unknown) => {
          recoveryRequired = true;
          if (retire) {
            retiring = false;
            closing = null;
          }
          if (live) error = t('decks.saveFailed');
          throw reason;
        })
        .finally(() => {
          queued--;
          publish();
        });
      tail = operation;
      if (retire) closing = operation;
      return operation;
    },
  };
  const share = (format: DeckcraftExportFormat): Promise<void> => {
    try {
      lifetime.check();
    } catch (error) {
      return Promise.reject(error);
    }
    if (!opened || retiring) return Promise.reject(new Error(t('decks.exportFailed')));
    queued++;
    publish();
    let resumed = false;
    const operation = tail
      .catch(() => {})
      .then(async () => {
        lifetime.check();
        await current.flushResource();
        lifetime.check();
        const resume = async () => {
          if (live && !retiring) {
            const response = await current.callAppTool('resume_document');
            lifetime.check();
            if (response.isError) throw new Error(t('decks.saveFailed'));
            resumed = true;
            recoveryRequired = false;
          }
        };
        try {
          const source = await options.driver.request({
            op: 'app_binary_read',
            item_id: options.itemId,
            version: null,
            expected_sha256: null,
          });
          lifetime.check();
          if (
            source.item_id !== options.itemId ||
            !Number.isSafeInteger(source.version) ||
            source.version < 1 ||
            !/^[a-f0-9]{64}$/.test(source.sha256)
          )
            throw new Error(t('decks.exportFailed'));
          await options.share(
            format,
            { version: String(source.version), sha256: source.sha256 },
            async (name, args) => {
              lifetime.check();
              const result = await current.callAppTool(name, args);
              lifetime.check();
              return result;
            },
          );
          lifetime.check();
        } catch (reason) {
          await resume().catch(() => {});
          throw reason;
        }
        await resume();
        phase = 'saved';
        if (resumed) {
          error = null;
          reason = null;
        }
      })
      .catch((reason: unknown) => {
        if (!resumed) recoveryRequired = true;
        if (live) error = t('decks.exportFailed');
        throw reason;
      })
      .finally(() => {
        queued--;
        publish();
      });
    tail = operation;
    return operation;
  };
  publish();
  return {
    controller,
    get opened() {
      return opened;
    },
    share,
    drain: () => tail.catch(() => {}),
    receive: (message: string) => current.receive(message),
    sync: () => current.sync(),
    syncContext: () => current.syncContext(),
    retire() {
      if (!live) return;
      live = false;
      lifetime.close();
      binary.dispose();
      current.dispose();
      options.disposeFrame();
    },
  };
}
