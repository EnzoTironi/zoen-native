import { t } from '../i18n/index.ts';
import { binaryEditorFormat, binaryExportAllowed, type BinaryEditorId } from './binary-formats.ts';
import type { CoreDriver } from '../core/types';
import type { VerifiedMiniappResource } from './sandbox';
import { createNativeCanonicalBinaryHost } from './canonical-binary-native.ts';
import { canonicalBinaryTools } from './canonical-binary.ts';
import { head, record } from './deckcraft-document-wire.ts';
import { MiniappSession } from './protocol.ts';
import { frameAuthority } from './lifetime.ts';
import type { MiniappDocumentController } from './document-controller';
import type { DeckcraftExportFormat, DeckcraftExportHead } from './deckcraft-export';
import { deckcraftFeedbackReason, type DeckcraftFeedbackReason } from './deckcraft-feedback.ts';

type Phase = 'saved' | 'saving' | 'pending' | 'error';
type Options = {
  editor?: BinaryEditorId;
  itemId: string;
  selectedHead?: DeckcraftExportHead;
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
  const descriptor = binaryEditorFormat(options.editor);
  const editor = descriptor.id;
  const messages = descriptor.messages;
  const lifetime = frameAuthority(options.check);
  // A history frame has no write authority, even if its guest requests save tools.
  const selected = options.selectedHead ? Object.freeze({ ...options.selectedHead }) : null;
  if (
    selected &&
    (!/^[1-9][0-9]{0,9}$/.test(selected.version) ||
      Number(selected.version) > 0xffffffff ||
      !/^[a-f0-9]{64}$/.test(selected.sha256))
  )
    throw Error(t(`${messages}.exportFailed`));
  const readDriver: Pick<CoreDriver, 'request'> = {
    request: (async (command) => {
      if (!selected || command.op !== 'app_binary_read') return options.driver.request(command);
      if (
        (command.version !== null && command.version !== Number(selected.version)) ||
        (command.expected_sha256 !== null && command.expected_sha256 !== selected.sha256)
      )
        throw Error(t(`${messages}.exportFailed`));
      const source = await options.driver.request({
        op: 'app_binary_read',
        item_id: options.itemId,
        version: Number(selected.version),
        expected_sha256: selected.sha256,
      });
      lifetime.check();
      if (source.version !== Number(selected.version) || source.sha256 !== selected.sha256)
        throw Error(t(`${messages}.exportFailed`));
      return source;
    }) as CoreDriver['request'],
  };
  const binary = createNativeCanonicalBinaryHost({
    editor,
    itemId: options.itemId,
    driver: readDriver,
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
          !binaryExportAllowed(editor, args.format) ||
          Object.keys(args).some((key) => !['id', 'format'].includes(key))
        )
          throw Error(t(`${messages}.exportFailed`));
        await share(args.format as DeckcraftExportFormat);
        lifetime.check();
        return { content: [], structuredContent: { presented: true } };
      }
      if (
        selected &&
        !['app_binary_read', 'app_binary_read_chunk', 'app_binary_export_source'].includes(name)
      )
        throw Error(t('documents.thisVersionIsReadOnly'));
      const result = await binary.call(name, args);
      if (selected && name === 'app_binary_read') {
        if (
          !result.structuredContent ||
          typeof result.structuredContent !== 'object' ||
          Array.isArray(result.structuredContent)
        )
          throw Error(t(`${messages}.exportFailed`));
        result.structuredContent = { ...result.structuredContent, readOnly: true };
      }
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
                ? t(`${messages}.editInProgress`)
                : reason === 'layout_failed' || reason === 'host_ui_failed'
                  ? t(`${messages}.viewFailed`)
                  : reason === 'export_failed'
                    ? t(`${messages}.exportFailed`)
                    : t(`${messages}.saveFailed`)
              : status.reason === 'graphics_unavailable'
                ? t(`${messages}.graphicsUnavailable`)
                : status.reason === 'canvas_missing'
                  ? t(`${messages}.canvasUnavailable`)
                  : status.reason === 'resource_fetch_failed'
                    ? t(`${messages}.resourceUnavailable`)
                    : status.reason === 'module_init_failed'
                      ? t(`${messages}.moduleUnavailable`)
                      : status.reason === 'document_open_failed'
                        ? t(`${messages}.documentUnavailable`)
                        : status.reason === 'runtime_start_failed'
                          ? t(`${messages}.runtimeUnavailable`)
                          : t(`${messages}.unavailable`)
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
    observedHead:
      editor === 'wordcraft' && !selected
        ? () => {
            try {
              lifetime.check();
              if (!opened || retiring) throw Error(t(`${messages}.saveFailed`));
            } catch (failure) {
              return Promise.reject(failure);
            }
            // A read of the installed Session shares ordering, never frame blocking.
            const operation = tail
              .catch(() => {})
              .then(async () => {
                lifetime.check();
                if (!opened || retiring) throw Error(t(`${messages}.saveFailed`));
                const response = await current.callAppTool('document_head', {});
                lifetime.check();
                if (retiring || response.isError) throw Error(t(`${messages}.saveFailed`));
                const value = record(response.structuredContent);
                const installed = record(value.head);
                if (
                  Object.keys(value).sort().join(',') !== 'head,itemId' ||
                  value.itemId !== options.itemId ||
                  Object.keys(installed).sort().join(',') !== 'sha256,version'
                )
                  throw Error(t(`${messages}.saveFailed`));
                const accepted = head(installed);
                return Object.freeze({
                  itemId: options.itemId,
                  version: Number(accepted.version),
                  sha256: accepted.sha256,
                });
              });
            tail = operation.then(
              () => {},
              () => {},
            );
            return operation;
          }
        : undefined,
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
      if (selected) {
        // Read-only Close has no save to perform, but must drain any native share.
        const operation = tail.catch(() => {}).then(() => lifetime.check());
        tail = operation;
        if (retire) closing = operation;
        publish();
        return operation;
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
            if (resumed.isError) throw new Error(t(`${messages}.saveFailed`));
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
          if (live) error = t(`${messages}.saveFailed`);
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
    if (!binaryExportAllowed(editor, format) || !opened || retiring)
      return Promise.reject(new Error(t(`${messages}.exportFailed`)));
    queued++;
    publish();
    let resumed = false;
    const operation = tail
      .catch(() => {})
      .then(async () => {
        lifetime.check();
        if (!selected) await current.flushResource();
        lifetime.check();
        const resume = async () => {
          if (live && !retiring) {
            if (selected) {
              resumed = true;
              recoveryRequired = false;
              return;
            }
            const response = await current.callAppTool('resume_document');
            lifetime.check();
            if (response.isError) throw new Error(t(`${messages}.saveFailed`));
            resumed = true;
            recoveryRequired = false;
          }
        };
        try {
          const source = await readDriver.request({
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
            throw new Error(t(`${messages}.exportFailed`));
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
      .catch((failure: unknown) => {
        if (!resumed && !selected) recoveryRequired = true;
        if (live) {
          error = t(`${messages}.exportFailed`);
          reason = 'export_failed';
        }
        throw failure;
      })
      .finally(() => {
        queued--;
        publish();
      });
    tail = operation;
    return operation;
  };
  const refresh = (observedVersion?: number): Promise<void> => {
    try {
      lifetime.check();
    } catch (failure) {
      return Promise.reject(failure);
    }
    // Higher-head Undo is qualified for Word; other editors retain their current lifecycle.
    if (editor !== 'wordcraft' || selected || !opened || retiring) return Promise.resolve();
    // A canonical notification is observational, not a user-requested pause.
    // Core publishes our save before its ACK reaches the guest. Blocking the
    // frame here can blur live input while the guest is waiting to recognize
    // that own head. The guest serializes refresh with saves and freezes only
    // when it actually needs to review a higher head; failures still fence recovery.
    const operation = tail
      .catch(() => {})
      .then(async () => {
        lifetime.check();
        if (retiring) return;
        const response = await current.callAppTool(
          'refresh_document',
          observedVersion === undefined ? {} : { version: observedVersion },
        );
        lifetime.check();
        if (response.isError) throw new Error(t(`${messages}.saveFailed`));
        if (!recoveryRequired) {
          phase = 'saved';
          error = null;
          reason = null;
        }
      })
      .catch((failure: unknown) => {
        // A local draft/pending receipt remains in the same Session for explicit recovery.
        recoveryRequired = true;
        if (live) error = t(`${messages}.saveFailed`);
        throw failure;
      })
      .finally(() => {
        publish();
      });
    tail = operation;
    return operation;
  };
  publish();
  return {
    controller,
    refresh,
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
