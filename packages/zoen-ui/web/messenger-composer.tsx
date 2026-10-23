import { useEffect, useRef, useState } from 'react';
import { message as reason } from './boundary.ts';
import type { Message, VoiceMeta } from './messages.ts';
import { kindOf, thumbnail, useRecorder } from './media.tsx';
import { Icon } from './messenger-presentation.tsx';

type Outgoing = { name: string; mime: string; bytes: Uint8Array; thumb?: Uint8Array | null; voice?: VoiceMeta };
export function Composer({ placeholder, disabled, reply, cancelReply, submit, autoFocus, attach }: {
  placeholder: string; disabled: boolean; reply?: Message | null; cancelReply?: () => void; autoFocus?: boolean;
  submit: (text: string) => Promise<void>; attach?: (file: Outgoing) => Promise<void>;
}) {
  const [sending, setSending] = useState('');
  const picker = useRef<HTMLInputElement>(null);
  const share = (out: Outgoing, label: string) => { if (!attach) return Promise.resolve(); setSending(label); setError('');
    return attach(out).catch((e: unknown) => setError(reason(e))).finally(() => setSending('')); };
  const files = async (list: FileList | File[]) => {
    for (const f of Array.from(list)) {
      const bytes = new Uint8Array(await f.arrayBuffer());
      await share({ name: f.name || 'arquivo', mime: f.type || 'application/octet-stream', bytes, thumb: await thumbnail(f) },
        kindOf(f.type) === 'image' ? 'Enviando foto…' : kindOf(f.type) === 'video' ? 'Enviando vídeo…' : 'Enviando arquivo…');
    }
  };
  const recorder = useRecorder((clip) => { if (clip.seconds < 0.6) return; void share({ name: `Mensagem de voz.${clip.mime.includes('mp4') ? 'm4a' : 'webm'}`, mime: clip.mime, bytes: clip.bytes,
    voice: { ms: Math.round(clip.seconds * 1000), levels: clip.levels, transcript: '', reply: reply?.entry ? { to: reply.entry, thread: false } : null } }, 'Enviando mensagem de voz…'); },
    (why) => setError(why));
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => { if (reply) field.current?.focus(); }, [reply?.id]);
  useEffect(() => { const el = field.current; if (el) { el.style.height = 'auto'; el.style.height = `${Math.min(el.scrollHeight, 168)}px`; } }, [draft]);
  const has = draft.trim().length > 0;
  function go() {
    if (!has || busy || disabled) return;
    setBusy(true); setError('');
    void submit(draft).then(() => setDraft('')).catch((e: unknown) => setError(reason(e))).finally(() => { setBusy(false); field.current?.focus(); });
  }
  return <div className="zm-composer-zone" onDragOver={(e) => { if (attach && e.dataTransfer.types.includes('Files')) e.preventDefault(); }}
    onDrop={(e) => { if (!attach || !e.dataTransfer.files.length) return; e.preventDefault(); void files(e.dataTransfer.files); }}>
    {reply ? <div className="zm-replying"><span><b>{`Respondendo a ${reply.name}`}</b>{reply.text}</span>
      <button aria-label="Cancelar" onClick={() => { setDraft(''); cancelReply?.(); }}><Icon name="close" /></button></div> : null}
    {error ? <p className="zm-error" role="alert">{error}</p> : null}
    {sending ? <p className="zm-sending" role="status">{sending}</p> : null}
    {recorder.recording ? <div className="zm-composer zm-recording" role="group" aria-label="Gravando mensagem de voz">
      <button type="button" className="zm-round" aria-label="Descartar gravação" onClick={recorder.cancel}><Icon name="close" /></button>
      <span className="zm-rec-dot" aria-hidden="true" /><span className="zm-voice-time">{recorder.clock(recorder.seconds)}</span>
      <span className="zm-wave zm-live" aria-hidden="true">{Array.from({ length: 28 }, (_, i) => recorder.recording!.levels[i - 28 + recorder.recording!.levels.length] ?? 0.08).map((p, i) => <i key={i} style={{ height: `${Math.round(p * 100)}%` }} />)}</span>
      <button type="button" className="zm-send glass zm-has" aria-label="Enviar mensagem de voz" onClick={recorder.send}><span className="zm-swap"><Icon name="send" /></span></button>
    </div> : <form className="zm-composer" onSubmit={(e) => { e.preventDefault(); go(); }}>
      {attach ? <><button type="button" className="zm-attach" aria-label="Anexar foto, vídeo ou arquivo" disabled={disabled} onClick={() => picker.current?.click()}><Icon name="plus" /></button>
        <input ref={picker} type="file" multiple hidden onChange={(e) => { if (e.target.files) void files(e.target.files); e.target.value = ''; }} /></> : null}
      <label className="zm-field"><span className="zw-sr-only">Mensagem</span>
        <textarea ref={field} rows={1} value={draft} disabled={disabled} placeholder={placeholder} maxLength={32_768} autoFocus={autoFocus}
          onChange={(e) => setDraft(e.target.value)}
          onPaste={(e) => { if (attach && e.clipboardData.files.length) { e.preventDefault(); void files(e.clipboardData.files); } }}
          onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); go(); } if (e.key === 'Escape') cancelReply?.(); }} /></label>
      <button type={has ? 'submit' : 'button'} key={has ? 'send' : 'mic'} className={`zm-send glass${has ? ' zm-has' : ''}`} disabled={disabled || busy || (!has && !attach)}
        aria-label={has ? 'Enviar' : 'Gravar mensagem de voz'} onClick={has ? undefined : () => void recorder.start()}>
        <span className="zm-swap"><Icon name={has ? 'send' : 'mic'} /></span></button>
    </form>}
  </div>;
}

