import { loadBinaryDocument } from './binary-document-load.ts';
import {
  token,
  digest,
  positive,
  headEqual,
  record,
  head,
  scope,
  scopeEqual,
  type Scope,
  type Head,
  type DeckcraftBindings,
} from './deckcraft-document-wire.ts';
export type { DeckcraftBindings } from './deckcraft-document-wire.ts';
import type { App } from '@modelcontextprotocol/ext-apps';
import { encode64, hash, CANONICAL_SCENE_CHUNK_BYTES as CHUNK } from './canonical-scene.ts';
import {
  binaryEditorFormat,
  binaryExportAllowed,
  type BinaryEditorId,
  type BinaryExportFormat,
} from './binary-formats.ts';
import { deckcraftInteractionRefusal, type DeckcraftFeedbackReason } from './deckcraft-feedback.ts';

type Options = {
  editor?: BinaryEditorId;
  app: Pick<App, 'callServerTool'>;
  engine: DeckcraftBindings;
  itemId: string;
  id: () => string;
  onState?: (
    phase: 'saved' | 'saving' | 'pending' | 'error',
    error?: unknown,
    reason?: DeckcraftFeedbackReason,
  ) => void;
};
/** Official MCP tools carry bounded document chunks. Local WASM calls target the same
 * rendered Session; only a durable native-core receipt can advance its accepted head. */
export class DeckcraftDocument {
  private live = true;
  private paused = false;
  private opened = false;
  private readOnly = false;
  private binding: Scope | null = null;
  private pending: { metadata: Record<string, unknown>; bytes: Uint8Array } | null = null;
  private tail: Promise<unknown> = Promise.resolve();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readonly options: Options;
  constructor(options: Options) {
    this.options = { ...options, editor: binaryEditorFormat(options.editor).id };
    if (!token(options.itemId)) throw new Error('Invalid document Item.');
  }
  private check() {
    if (!this.live) throw new Error('This document has closed.');
  }
  private serial<T>(work: () => Promise<T>): Promise<T> {
    const task = this.tail
      .catch(() => {})
      .then(async () => {
        this.check();
        return work();
      });
    this.tail = task;
    return task;
  }
  private state(
    phase: 'saved' | 'saving' | 'pending' | 'error',
    error?: unknown,
    reason?: DeckcraftFeedbackReason,
  ) {
    if (!this.live) return;
    try {
      this.options.onState?.(phase, error, reason);
    } catch {
      /* Presentation cannot change a durable receipt. */
    }
  }
  private async tool(name: string, args: Record<string, unknown>) {
    this.check();
    const response = await this.options.app.callServerTool({
      name,
      arguments: { id: this.options.itemId, ...args },
    });
    this.check();
    if (response.isError) throw new Error('The document operation was refused.');
    return record(response.structuredContent);
  }
  private async engine(op: string, input?: unknown, bytes = new Uint8Array()) {
    this.check();
    // Check before wasm-bindgen copies the Uint8Array or its JSON String.
    if (
      !(bytes instanceof Uint8Array) ||
      bytes.byteLength > binaryEditorFormat(this.options.editor).maxDocumentBytes
    )
      throw new Error('Document input is too large.');
    const envelope = JSON.stringify(input === undefined ? { op } : { op, input });
    if (envelope.length > 65_536 || new TextEncoder().encode(envelope).length > 65_536)
      throw new Error('Document command is too large.');
    const response = await this.options.engine.zoenHostRequest(envelope, bytes);
    this.check();
    if (
      typeof response.metadata !== 'string' ||
      response.metadata.length > 65_536 ||
      !(response.bytes instanceof Uint8Array) ||
      response.bytes.byteLength >
        (op === 'export'
          ? 16 * 1024 * 1024
          : binaryEditorFormat(this.options.editor).maxDocumentBytes)
    )
      throw new Error('Invalid engine response.');
    const metadata = record(JSON.parse(response.metadata));
    if (metadata.engineRevision !== binaryEditorFormat(this.options.editor).engineRevision)
      throw new Error('The document engine revision changed.');
    if (this.binding && !scopeEqual(scope(metadata.scope, this.options.itemId), this.binding))
      throw new Error('The document authority changed.');
    return { metadata, bytes: response.bytes };
  }
  open(): Promise<void> {
    return this.serial(async () => {
      if (this.opened) throw new Error('The document is already open.');
      const { read, binding, bytes } = await loadBinaryDocument(
        this.options.editor,
        this.options.itemId,
        (name, args) => this.tool(name, args),
      );
      this.binding = binding;
      try {
        await this.engine(
          'open',
          { scope: this.binding, head: { version: String(read.version), sha256: read.sha256 } },
          bytes,
        );
        this.readOnly = read.readOnly === true;
        if (this.readOnly) {
          if (!(await this.engine('freeze', { frozen: true })).metadata.frozen)
            throw new Error('The historical document did not acknowledge read-only mode.');
          this.paused = true;
        }
        this.opened = true;
        this.options.engine.zoenHostOnChange((encoded) => {
          if (!this.live || this.paused || this.readOnly) return;
          try {
            if (record(JSON.parse(encoded)).dirty) this.schedule();
          } catch (error) {
            this.state('error', error, 'document_status_failed');
          }
        });
        this.state('saved');
      } finally {
        bytes.fill(0);
      }
    });
  }
  /** Read only the accepted head installed in this exact live Session. */
  observedHead(): Promise<{ itemId: string; head: Head }> {
    return this.serial(async () => {
      if (!this.opened || !this.binding) throw Error('The document is not open.');
      const status = (await this.engine('status')).metadata;
      if (this.pending || status.dirty !== false || status.pendingMutationId != null)
        throw Error('The document has unacknowledged changes.');
      return { itemId: this.options.itemId, head: head(status.head) };
    });
  }

  /** Receive a canonical higher head on this Session, never overwriting a local draft. */
  refresh(observedVersion?: number): Promise<void> {
    return this.serial(async () => {
      if (!this.opened || !this.binding) throw new Error('The document is not open.');
      if (this.readOnly) throw new Error('This historical document is read-only.');
      if (observedVersion !== undefined) {
        if (!positive(observedVersion)) throw new Error('Invalid document version notification.');
        // Own receipts and stale notifications must not pause newer admitted human input.
        const accepted = head((await this.engine('status')).metadata.head);
        if (Number(accepted.version) >= observedVersion) return;
      }
      const wasPaused = this.paused;
      this.paused = true;
      if (this.timer) clearTimeout(this.timer);
      this.timer = null;
      try {
        // The engine acknowledges only after admitted input/composition has drained.
        const paused = (await this.engine('freeze', { frozen: true })).metadata;
        if (!paused.frozen) throw new Error('The editor did not acknowledge pause.');
        const status = (await this.engine('status')).metadata;
        if (this.pending || status.dirty || status.pendingMutationId)
          throw new Error('Preserve the local draft before reviewing the new version.');
        const loaded = await loadBinaryDocument(
          this.options.editor,
          this.options.itemId,
          (name, args) => this.tool(name, args),
        );
        try {
          if (loaded.read.readOnly === true || !scopeEqual(loaded.binding, this.binding))
            throw new Error('The document authority changed.');
          const next = { version: String(loaded.read.version), sha256: String(loaded.read.sha256) };
          const prior = head(status.head);
          if (!headEqual(prior, next)) {
            if (Number(next.version) <= Number(prior.version))
              throw new Error('The canonical document did not advance.');
            await this.engine('open', { scope: this.binding, head: next }, loaded.bytes);
          }
          this.state('saved');
        } finally {
          loaded.bytes.fill(0);
        }
      } finally {
        // An in-flight close/authority retirement must never revive an old owner.
        if (this.live && !wasPaused) {
          await this.engine('freeze', { frozen: false });
          this.paused = false;
        }
      }
    });
  }
  private schedule() {
    if (!this.live || this.paused || this.timer) return;
    this.state('pending');
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.save().catch((error: unknown) =>
        this.state(
          'error',
          error,
          deckcraftInteractionRefusal(error) ? 'autosave_busy' : 'autosave_failed',
        ),
      );
    }, 220);
  }
  private async persist() {
    const pending = this.pending;
    if (!pending || !this.binding) return;
    const p = pending.metadata;
    const base = head(p.base);
    if (
      !token(p.mutationId) ||
      !digest(p.sha256) ||
      p.byteLength !== pending.bytes.length ||
      hash(pending.bytes) !== p.sha256 ||
      !scopeEqual(scope(p.scope, this.options.itemId), this.binding)
    )
      throw new Error('The pending proposal changed.');
    this.state('saving');
    const intentSha256 = hash(
      new TextEncoder().encode(JSON.stringify({ kind: 'humanCheckpoint' })),
    );
    const begin = await this.tool('app_binary_begin', {
      mutationId: p.mutationId,
      version: Number(base.version),
      sha256: base.sha256,
      bytes: pending.bytes.length,
      documentSha256: p.sha256,
      intentSha256,
    });
    if (!token(begin.uploadId) || begin.chunkBytes !== CHUNK)
      throw new Error('Invalid document upload.');
    for (let index = 0; index < Math.ceil(pending.bytes.length / CHUNK); index++) {
      const reply = await this.tool('app_binary_append', {
        uploadId: begin.uploadId,
        index,
        data: encode64(pending.bytes.subarray(index * CHUNK, (index + 1) * CHUNK)),
      });
      if (reply.nextIndex !== index + 1)
        throw new Error('Invalid document upload acknowledgement.');
    }
    const receipt = await this.tool('app_binary_commit', { uploadId: begin.uploadId });
    const accepted = head(receipt.head);
    if (
      receipt.mutationId !== p.mutationId ||
      accepted.version !== String(Number(base.version) + 1) ||
      accepted.sha256 !== p.sha256 ||
      !positive(receipt.currentVersion) ||
      Number(receipt.currentVersion) < Number(accepted.version) ||
      receipt.engineRevision !== binaryEditorFormat(this.options.editor).engineRevision
    )
      throw new Error('The save did not accept this proposal.');
    await this.engine('accept', { scope: this.binding, mutationId: p.mutationId, head: accepted });
    pending.bytes.fill(0);
    if (this.pending === pending) this.pending = null;
    if (receipt.currentVersion !== Number(accepted.version))
      throw new Error(
        'A peer advanced this document; preserve the draft and review the new version.',
      );
  }
  private async saveCurrent() {
    if (!this.opened || !this.binding) throw new Error('The document is not open.');
    if (this.readOnly) throw new Error('This historical document is read-only.');
    // A failed/lost response retries the exact old bytes and mutation before taking a new snapshot.
    await this.persist();
    for (let attempt = 0; attempt < 3; attempt++) {
      const status = (await this.engine('status')).metadata;
      if (!status.dirty) {
        this.state('saved');
        return;
      }
      const mutationId = this.options.id();
      if (!token(mutationId)) throw new Error('Invalid document mutation identifier.');
      const proposal = await this.engine('snapshot', {
        scope: this.binding,
        base: head(status.head),
        mutationId,
      });
      this.pending = { metadata: proposal.metadata, bytes: proposal.bytes.slice() };
      await this.persist();
    }
    if ((await this.engine('status')).metadata.dirty)
      throw new Error('The document is still changing; finish editing before closing.');
    this.state('saved');
  }
  save(): Promise<void> {
    return this.serial(() => this.saveCurrent());
  }
  /** Teardown pauses instead of destroying the Session: failed close preserves the
   * original draft, and a background flush can explicitly resume the same document. */
  flush(deferInteraction = false): Promise<void> {
    return this.serial(async () => {
      if (this.readOnly) return;
      this.paused = true;
      if (this.timer) {
        clearTimeout(this.timer);
        this.timer = null;
      }
      try {
        if (!(await this.engine('freeze', { frozen: true })).metadata.frozen)
          throw new Error('The editor did not acknowledge pause.');
        await this.saveCurrent();
        const status = (await this.engine('status')).metadata;
        if (!status.frozen || status.dirty || status.pendingMutationId)
          throw new Error('The paused document still has unsaved changes.');
      } catch (error) {
        if (this.live) {
          await this.engine('freeze', { frozen: false }).catch(() => {});
          this.paused = false;
        }
        if (deferInteraction && deckcraftInteractionRefusal(error)) this.state('pending');
        else
          this.state(
            'error',
            error,
            deckcraftInteractionRefusal(error) ? 'flush_busy' : 'flush_failed',
          );
        throw error;
      }
    });
  }
  /** A lifecycle interruption cannot certify input retained by a hidden UI frame. */
  async checkpoint(): Promise<boolean> {
    try {
      await this.flush(true);
    } catch (error) {
      if (deckcraftInteractionRefusal(error)) return false;
      throw error;
    }
    await this.resume();
    return true;
  }
  resume(): Promise<void> {
    return this.serial(async () => {
      if (this.readOnly) return;
      await this.engine('freeze', { frozen: false });
      this.paused = false;
      if ((await this.engine('status')).metadata.dirty) this.schedule();
    });
  }
  export(format: BinaryExportFormat, selected: Head) {
    return this.serial(async () => {
      if (!binaryExportAllowed(this.options.editor ?? 'deckcraft', format))
        throw new Error('Invalid document export format.');
      if (!this.binding) throw new Error('The document is not open.');
      selected = head(selected);
      const source = await this.tool('app_binary_export_source', {
        version: Number(selected.version),
        sha256: selected.sha256,
      });
      if (source.version !== Number(selected.version) || source.sha256 !== selected.sha256)
        throw new Error('The export source changed.');
      const status = (await this.engine('status')).metadata;
      if (!headEqual(head(status.head), selected))
        throw new Error('Open the selected accepted version before exporting it.');
      const output = await this.engine('export', { scope: this.binding, head: selected, format });
      if (
        !headEqual(head(output.metadata.sourceHead), selected) ||
        !digest(output.metadata.sha256) ||
        output.metadata.sha256 !== hash(output.bytes) ||
        output.metadata.byteLength !== output.bytes.length ||
        output.metadata.format !== format
      )
        throw new Error('Invalid accepted-version export.');
      return output;
    });
  }
  /** Owner/access retirement only. Ordinary dismissal must await flush first. */
  retireAuthority() {
    if (!this.live) return;
    this.live = false;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.pending?.bytes.fill(0);
    this.pending = null;
    try {
      this.options.engine.zoenHostOnChange(undefined);
    } finally {
      this.options.engine.zoenHostRetire();
    }
  }
}

/** Register only standard official App SDK callbacks after connect/load. Export
 * chunks stay inside this artifact binding; native sharing is a separate user action. */
export async function attachDeckcraftDocument(options: Options & { app: App }) {
  const document = new DeckcraftDocument(options);
  let exported: { id: string; bytes: Uint8Array } | null = null;
  const clearExport = () => {
    exported?.bytes.fill(0);
    exported = null;
  };
  // Keep the bootstrap's cancellation-safe pre-open handlers until a document exists.
  // A failed initial load has no accepted manual draft and must not retain a flush
  // callback targeting an unopened Session or prevent the loading screen from closing.
  try {
    await document.open();
  } catch (error) {
    clearExport();
    document.retireAuthority();
    throw error;
  }
  options.app.onteardown = async () => {
    await document.flush();
    clearExport();
    return {};
  };
  options.app.oncalltool = async ({ name, arguments: input }) => {
    const args = record(input ?? {});
    if (name === 'checkpoint_document') {
      if (Object.keys(args).length) throw Error('Invalid document checkpoint request.');
      return { content: [], structuredContent: { saved: await document.checkpoint() } };
    }
    if (name === 'document_head') {
      if (Object.keys(args).length) throw Error('Invalid document head request.');
      return { content: [], structuredContent: await document.observedHead() };
    }
    if (name === 'refresh_document') {
      if (
        Object.keys(args).some((key) => key !== 'version') ||
        (args.version !== undefined && !positive(args.version))
      )
        throw new Error('Invalid document version notification.');
      await document.refresh(args.version === undefined ? undefined : Number(args.version));
      clearExport();
      return { content: [] };
    }
    if (name === 'resume_document') {
      await document.resume();
      return { content: [] };
    }
    if (name === 'flush_document') {
      await document.flush();
      return { content: [] };
    }
    if (name === 'prepare_document_export') {
      if (!binaryExportAllowed(options.editor ?? 'deckcraft', args.format))
        throw new Error('Invalid document export format.');
      const output = await document.export(args.format, head(args.head));
      clearExport();
      const id = options.id();
      if (!token(id)) throw new Error('Invalid export identifier.');
      exported = { id, bytes: output.bytes };
      return {
        content: [],
        structuredContent: {
          transferId: id,
          bytes: exported.bytes.length,
          chunkBytes: CHUNK,
          ...output.metadata,
        },
      };
    }
    if (name === 'read_document_export_chunk') {
      if (
        !exported ||
        args.transferId !== exported.id ||
        typeof args.index !== 'number' ||
        !Number.isSafeInteger(args.index) ||
        args.index < 0 ||
        args.index >= Math.ceil(exported.bytes.length / CHUNK)
      )
        throw new Error('Invalid document export chunk.');
      return {
        content: [],
        structuredContent: {
          index: args.index,
          data: encode64(exported.bytes.subarray(args.index * CHUNK, (args.index + 1) * CHUNK)),
        },
      };
    }
    if (name === 'release_document_export') {
      if (!exported || args.transferId !== exported.id)
        throw new Error('Invalid document export identifier.');
      clearExport();
      return { content: [] };
    }
    throw new Error('This document tool is unavailable.');
  };
  return {
    document,
    retireAuthority: () => {
      clearExport();
      document.retireAuthority();
    },
  };
}
