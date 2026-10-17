import { bytes, hex, object } from './boundary.ts';
import { parseSession, type StoredSession } from './session.ts';
import {parseAuthority,type SealedAuthority} from './authority.ts';

const DB = 'zoen-browser-v1';
const AAD = new TextEncoder().encode('zoen-browser-session-v1');

export async function sealSession(session: StoredSession, key: CryptoKey) {
  const plain = new TextEncoder().encode(JSON.stringify(session));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  try {
    const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: AAD }, key, plain);
    return { iv, ciphertext: new Uint8Array(ciphertext) };
  } finally { plain.fill(0); }
}

export async function openSession(encrypted: unknown, key: CryptoKey): Promise<StoredSession> {
  const record = object(encrypted);
  const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: bytes(record.iv), additionalData: AAD }, key, bytes(record.ciphertext));
  try { return parseSession(new TextDecoder('utf-8', { fatal: true }).decode(plain)); }
  finally { new Uint8Array(plain).fill(0); }
}

function request<T>(operation: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    operation.onsuccess = () => resolve(operation.result);
    operation.onerror = () => reject(operation.error ?? new Error('Browser storage failed.'));
  });
}

function completed(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onabort = () => reject(transaction.error ?? new Error('Browser storage transaction aborted.'));
    transaction.onerror = () => reject(transaction.error ?? new Error('Browser storage transaction failed.'));
  });
}

function validKey(value: unknown, algorithm: string, type: KeyType): CryptoKey {
  if (!(value instanceof CryptoKey) || value.algorithm.name !== algorithm || value.type !== type ||
      (type !== 'public' && value.extractable)) {
    throw new Error('Stored browser keys are invalid. Pair this browser again.');
  }
  return value;
}

export class BrowserVault {
  private constructor(
    db: IDBDatabase, signingKey: CryptoKey, publicSigningKey: CryptoKey, sealingKey: CryptoKey,
  ) {
    this.db = db;
    this.signingKey = signingKey;
    this.publicSigningKey = publicSigningKey;
    this.sealingKey = sealingKey;
  }
  private terminal = false;
  private erasure: Promise<void> | null = null;
  private active(): void {
    if (this.terminal) throw new Error('This browser vault is closed.');
  }
  private readonly db: IDBDatabase;
  private readonly signingKey: CryptoKey;
  private readonly publicSigningKey: CryptoKey;
  private readonly sealingKey: CryptoKey;

  static async open(): Promise<BrowserVault> {
    const opening = indexedDB.open(DB, 1);
    opening.onupgradeneeded = () => opening.result.createObjectStore('vault');
    const db = await request(opening);
    const reading = db.transaction('vault', 'readonly');
    const readDone = completed(reading);
    const store = reading.objectStore('vault');
    const keysRead = request(store.get('keys'));
    const sessionRead = request(store.get('session'));
    const authorityRead=request(store.get('authority'));
    const [keys, session, authority]: [unknown, unknown,unknown] = await Promise.all([keysRead, sessionRead,authorityRead]);
    await readDone;
    if (keys !== undefined) {
      const record = object(keys);
      return new BrowserVault(db, validKey(record.privateKey, 'Ed25519', 'private'),
        validKey(record.publicKey, 'Ed25519', 'public'), validKey(record.sealingKey, 'AES-GCM', 'secret'));
    }
    if (session !== undefined||authority!==undefined) { db.close(); throw new Error('This browser session is missing its protected keys. Restore the browser data or pair a new device.'); }
    const signing = await crypto.subtle.generateKey('Ed25519', false, ['sign', 'verify']);
    if (!('privateKey' in signing)) throw new Error('This browser cannot create an Ed25519 device.');
    const sealingKey = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
    const transaction = db.transaction('vault', 'readwrite');
    const done = completed(transaction);
    // add refuses a racing creator; callers hold the exclusive Web Lock for this vault.
    transaction.objectStore('vault').add({ privateKey: signing.privateKey, publicKey: signing.publicKey, sealingKey }, 'keys');
    await done;
    return new BrowserVault(db, signing.privateKey, signing.publicKey, sealingKey);
  }

  async device(): Promise<string> {
    this.active();
    return hex(new Uint8Array(await crypto.subtle.exportKey('raw', this.publicSigningKey)));
  }

  /** Structured-cloned into the worker; the nonextractable key never becomes bytes. */
  authorityKey():CryptoKey{this.active();return this.sealingKey;}

  async authority():Promise<SealedAuthority|null>{
    this.active();
    const transaction=this.db.transaction('vault','readonly');const done=completed(transaction);
    const value:unknown=await request(transaction.objectStore('vault').get('authority'));await done;
    return value===undefined?null:parseAuthority(value);
  }

  async preparePrimary(session:Extract<StoredSession,{kind:'provider_pending'}>,authority:SealedAuthority,signal?:AbortSignal):Promise<void>{
    this.active();signal?.throwIfAborted();const record=parseAuthority(authority);
    if(record.device!==await this.device()||record.identity!==session.account.identity||record.device!==session.account.device)throw new Error('A conta mudou durante a entrada.');
    const encrypted=await sealSession(session,this.sealingKey);this.active();signal?.throwIfAborted();
    const transaction=this.db.transaction('vault','readwrite',{durability:'strict'});const done=completed(transaction);
    const abort=()=>{try{transaction.abort();}catch{}};signal?.addEventListener('abort',abort,{once:true});
    try{const store=transaction.objectStore('vault');store.add(record,'authority');store.put(encrypted,'session');await done;}
    finally{signal?.removeEventListener('abort',abort);}
  }

  async sign(payload: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
    this.active();
    const signature = new Uint8Array(await crypto.subtle.sign('Ed25519', this.signingKey, Uint8Array.from(payload)));
    if (this.terminal) { signature.fill(0); this.active(); }
    return signature;
  }

  async load(): Promise<StoredSession> {
    this.active();
    const transaction = this.db.transaction('vault', 'readonly');
    const done = completed(transaction);
    const encrypted: unknown = await request(transaction.objectStore('vault').get('session'));
    await done;
    if (encrypted === undefined) return { kind: 'empty' };
    return openSession(encrypted, this.sealingKey);
  }

  async save(session: StoredSession, signal?: AbortSignal): Promise<void> {
    this.active();
    signal?.throwIfAborted();
    const encrypted = await sealSession(session, this.sealingKey);
    this.active();
    signal?.throwIfAborted();
    const transaction = this.db.transaction('vault', 'readwrite', { durability: 'strict' });
    const done = completed(transaction);
    const abort = () => { try { transaction.abort(); } catch { /* Already committed or aborted. */ } };
    signal?.addEventListener('abort', abort, { once: true });
    try {
      transaction.objectStore('vault').put(encrypted, 'session');
      await done;
    } finally { signal?.removeEventListener('abort', abort); }
  }

  close(): void { this.terminal = true; this.db.close(); }

  erase(): Promise<void> {
    // Fence encryption already in flight before awaiting any storage work. A
    // queued write transaction finishes before this clear; a later write is refused.
    this.terminal = true;
    if (this.erasure) return this.erasure;
    this.erasure = (async () => {
      const transaction = this.db.transaction('vault', 'readwrite', { durability: 'strict' });
      const done = completed(transaction);
      transaction.objectStore('vault').clear();
      await done;
      this.db.close();
    })().catch(error => { this.erasure = null; throw error; });
    return this.erasure;
  }
}
