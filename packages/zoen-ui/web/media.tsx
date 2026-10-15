import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { message as reason } from './boundary.ts';
import type { Chunk, FileDoc, Messages } from './messages.ts';

/** The sender picks the type. A blob: URL shares our origin, so only inert media types
 *  keep theirs; HTML, SVG and the rest open as plain bytes and can only be downloaded. */
const safeMime = (mime: string) => /^(image\/(png|jpeg|gif|webp|avif|heic)|video\/(mp4|webm|quicktime)|audio\/(webm|mp4|mpeg|ogg|aac|wav))$/.test(mime) ? mime : 'application/octet-stream';

/** Decrypted files live only in memory, once per file, for this tab. */
const opened = new Map<string, Promise<string>>();
function openUrl(messages: Messages, key: string, chunks: Chunk[], mime: string): Promise<string> {
  let url = opened.get(key);
  if (!url) {
    url = messages.openFile(chunks, safeMime(mime)).then((blob) => URL.createObjectURL(blob));
    url.catch(() => opened.delete(key));
    opened.set(key, url);
  }
  return url;
}
function useOpened(messages: Messages | null, key: string, chunks: Chunk[] | null, mime: string, enabled = true) {
  const [state, setState] = useState<{ url?: string; error?: string }>({});
  useEffect(() => {
    if (!messages || !chunks || !enabled) return;
    let live = true;
    openUrl(messages, key, chunks, mime).then((url) => live && setState({ url })).catch((e: unknown) => live && setState({ error: reason(e) }));
    return () => { live = false; };
  }, [messages, key, enabled]);
  return state;
}

export const kindOf = (mime: string) => mime.startsWith('image/') ? 'image' : mime.startsWith('video/') ? 'video' : mime.startsWith('audio/') ? 'audio' : 'file';
const size = (n: number) => n < 1024 ? `${n} B` : n < 1 << 20 ? `${Math.round(n / 1024)} KB` : `${(n / (1 << 20)).toFixed(1).replace('.', ',')} MB`;
const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;

export function FileView({ file, messages }: { file: FileDoc; messages: Messages | null }) {
  const kind = kindOf(file.mime);
  if (kind === 'audio') return <VoicePlayer file={file} messages={messages} />;
  if (kind === 'image' || kind === 'video') return <Visual file={file} messages={messages} kind={kind} />;
  return <FileCard file={file} messages={messages} />;
}

function Visual({ file, messages, kind }: { file: FileDoc; messages: Messages | null; kind: 'image' | 'video' }) {
  const [full, setFull] = useState(false);
  const small = kind === 'image' && file.bytes <= 2 << 20;
  const thumb = useOpened(messages, `${file.sha256}:t`, file.thumb ? [file.thumb] : small ? file.chunks : null, file.thumb ? 'image/png' : file.mime);
  const whole = useOpened(messages, file.sha256, file.chunks, file.mime, full || (small && !file.thumb));
  const preview = thumb.url ?? (small ? whole.url : undefined);
  return <>
    <button className={`zm-visual zm-${kind}`} onClick={() => setFull(true)} aria-label={kind === 'image' ? `Abrir foto ${file.name}` : `Assistir vídeo ${file.name}`}>
      {preview ? <img src={preview} alt="" /> : <span className="zm-visual-wait">{thumb.error ?? whole.error ?? (kind === 'image' ? 'Carregando foto…' : 'Vídeo')}</span>}
      {kind === 'video' ? <span className="zm-play" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="M9 7l8 5-8 5z" /></svg></span> : null}
    </button>
    {full ? createPortal(<div className="zm-scrim zm-lightbox" role="dialog" aria-label={file.name} onClick={() => setFull(false)}>
      {whole.url ? kind === 'image' ? <img src={whole.url} alt={file.name} /> : <video src={whole.url} controls autoPlay onClick={(e) => e.stopPropagation()} /> :
        <p className="zm-note">{whole.error ?? 'Abrindo…'}</p>}
      <span className="zm-lightbox-bar" onClick={(e) => e.stopPropagation()}>{whole.url ? <a className="zm-pill" href={whole.url} download={file.name}>Baixar</a> : null}
        <button className="zm-pill" autoFocus onKeyDown={(e) => { if (e.key === 'Escape') setFull(false); }} onClick={() => setFull(false)}>Fechar</button></span>
    </div>, document.body) : null}
  </>;
}

function FileCard({ file, messages }: { file: FileDoc; messages: Messages | null }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const ext = (file.name.split('.').pop() ?? '').slice(0, 4).toUpperCase() || 'ARQ';
  return <span className="zm-file">
    <span className="zm-file-icon" aria-hidden="true">{ext}</span>
    <span className="zm-file-body"><b>{file.name}</b><span>{error || size(file.bytes)}</span></span>
    <button className="zm-round glass" aria-label={`Baixar ${file.name}`} disabled={busy || !messages} onClick={() => {
      if (!messages) return; setBusy(true); setError('');
      void openUrl(messages, file.sha256, file.chunks, file.mime).then((url) => { const a = document.createElement('a'); a.href = url; a.download = file.name; a.click(); })
        .catch((e: unknown) => setError(reason(e))).finally(() => setBusy(false));
    }}><svg className="zm-icon" viewBox="0 0 24 24"><path d="M12 4v11m0 0-4-4m4 4 4-4M5 20h14" /></svg></button>
  </span>;
}

/** Peak per bar, 0…1, from the decoded audio. */
async function levels(url: string, bars = 36): Promise<{ peaks: number[]; seconds: number }> {
  const data = await (await fetch(url)).arrayBuffer();
  const context = new OfflineAudioContext(1, 1, 44_100);
  const audio = await context.decodeAudioData(data);
  const samples = audio.getChannelData(0);
  const step = Math.max(1, Math.floor(samples.length / bars));
  const peaks = Array.from({ length: bars }, (_, i) => { let m = 0; for (let j = i * step; j < Math.min(samples.length, (i + 1) * step); j += 16) m = Math.max(m, Math.abs(samples[j]!)); return m; });
  const top = Math.max(...peaks, 0.01);
  return { peaks: peaks.map((p) => Math.max(0.08, p / top)), seconds: audio.duration };
}

export function VoicePlayer({ file, messages }: { file: FileDoc; messages: Messages | null }) {
  const opened = useOpened(messages, file.sha256, file.chunks, file.mime);
  const [wave, setWave] = useState<{ peaks: number[]; seconds: number } | null>(null);
  const [playing, setPlaying] = useState(false);
  const [at, setAt] = useState(0);
  const audio = useRef<HTMLAudioElement>(null);
  useEffect(() => { if (opened.url) void levels(opened.url).then(setWave).catch(() => setWave({ peaks: Array(36).fill(0.3), seconds: 0 })); }, [opened.url]);
  const duration = wave?.seconds || audio.current?.duration || 0;
  const progress = duration ? at / duration : 0;
  return <span className="zm-voice" data-testid="voice">
    <button className="zm-voice-play" aria-label={playing ? 'Pausar' : 'Ouvir mensagem de voz'} disabled={!opened.url} onClick={() => {
      const a = audio.current; if (!a) return; if (a.paused) void a.play(); else a.pause();
    }}>{playing ? <svg viewBox="0 0 24 24"><path d="M8 6h3v12H8zM13 6h3v12h-3z" /></svg> : <svg viewBox="0 0 24 24"><path d="M8 5.5l11 6.5-11 6.5z" /></svg>}</button>
    <span className="zm-wave" role="slider" aria-label="Posição" aria-valuemin={0} aria-valuemax={Math.round(duration)} aria-valuenow={Math.round(at)} tabIndex={0}
      onClick={(e) => { const a = audio.current; if (!a || !duration) return; const r = e.currentTarget.getBoundingClientRect(); a.currentTime = ((e.clientX - r.left) / r.width) * duration; }}>
      {(wave?.peaks ?? Array(36).fill(0.15)).map((p, i, all) => <i key={i} style={{ height: `${Math.round(p * 100)}%` }} className={i / all.length < progress ? 'zm-on' : ''} />)}
    </span>
    <span className="zm-voice-time">{opened.error ? 'Indisponível' : clock(playing || at ? at : duration)}</span>
    {opened.url ? <audio ref={audio} src={opened.url} preload="metadata" onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)}
      onEnded={() => { setPlaying(false); setAt(0); }} onTimeUpdate={(e) => setAt(e.currentTarget.currentTime)} /> : null}
  </span>;
}

/** A small PNG preview made on this device, like native, for photos and videos. */
export async function thumbnail(file: File): Promise<Uint8Array | null> {
  try {
    let source: CanvasImageSource; let w: number; let h: number;
    if (file.type.startsWith('image/')) { const b = await createImageBitmap(file); source = b; w = b.width; h = b.height; }
    else if (file.type.startsWith('video/')) {
      const v = document.createElement('video'); v.muted = true; v.playsInline = true; v.src = URL.createObjectURL(file);
      await new Promise<void>((ok, fail) => { v.onloadeddata = () => ok(); v.onerror = () => fail(new Error('video')); });
      v.currentTime = Math.min(0.5, v.duration / 2 || 0); await new Promise((ok) => { v.onseeked = ok; });
      source = v; w = v.videoWidth; h = v.videoHeight;
    } else return null;
    const scale = Math.min(1, 480 / Math.max(w, h));
    const canvas = new OffscreenCanvas(Math.round(w * scale), Math.round(h * scale));
    canvas.getContext('2d')!.drawImage(source, 0, 0, canvas.width, canvas.height);
    const png = new Uint8Array(await (await canvas.convertToBlob({ type: 'image/png' })).arrayBuffer());
    return png.length <= 2 << 20 ? png : null;
  } catch { return null; }
}

/** Hold-free voice recording (tap mic, tap send), with live levels. */
export function useRecorder(done: (clip: { bytes: Uint8Array; mime: string; seconds: number }) => void, fail: (why: string) => void) {
  const [recording, setRecording] = useState<{ started: number; levels: number[] } | null>(null);
  const parts = useRef<{ recorder: MediaRecorder; stream: MediaStream; chunks: Blob[]; keep: boolean; raf: number; context: AudioContext } | null>(null);
  const [now, setNow] = useState(0);
  const live = recording !== null;
  useEffect(() => { if (!live) return; const t = setInterval(() => setNow(Date.now()), 250); return () => clearInterval(t); }, [live]);
  async function start() {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mime = ['audio/webm;codecs=opus', 'audio/mp4', 'audio/webm'].find((m) => MediaRecorder.isTypeSupported(m)) ?? '';
      const recorder = new MediaRecorder(stream, mime ? { mimeType: mime } : {});
      const context = new AudioContext(); void context.resume(); const analyser = context.createAnalyser(); analyser.fftSize = 256;
      context.createMediaStreamSource(stream).connect(analyser);
      const buffer = new Uint8Array(analyser.frequencyBinCount);
      const state = { recorder, stream, chunks: [] as Blob[], keep: false, raf: 0, context };
      parts.current = state;
      const started = Date.now();
      const tick = () => { analyser.getByteTimeDomainData(buffer); let m = 0; for (const v of buffer) m = Math.max(m, Math.abs(v - 128) / 128);
        setRecording((r) => r && { ...r, levels: [...r.levels.slice(-27), Math.max(0.08, Math.min(1, m * 2.4))] }); state.raf = requestAnimationFrame(tick); };
      recorder.ondataavailable = (e) => { if (e.data.size) state.chunks.push(e.data); };
      recorder.onstop = () => {
        cancelAnimationFrame(state.raf); stream.getTracks().forEach((t) => t.stop()); void context.close();
        if (!state.keep) return;
        const blob = new Blob(state.chunks, { type: recorder.mimeType || 'audio/webm' });
        void blob.arrayBuffer().then((b) => done({ bytes: new Uint8Array(b), mime: (recorder.mimeType || 'audio/webm').split(';')[0]!, seconds: (Date.now() - started) / 1000 }));
      };
      recorder.start(250); setRecording({ started, levels: [] }); setNow(started); tick();
    } catch { fail('Libere o microfone no navegador para gravar.'); }
  }
  function stop(keep: boolean) { const p = parts.current; if (!p) return; p.keep = keep; p.recorder.stop(); parts.current = null; setRecording(null); }
  return { recording, seconds: recording ? (now - recording.started) / 1000 : 0, start, send: () => stop(true), cancel: () => stop(false), clock };
}
