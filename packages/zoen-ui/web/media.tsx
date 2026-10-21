import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { acquireUrl } from './media-url.ts';
import { message as reason } from './boundary.ts';
import type { FileDoc, Messages } from './messages.ts';

/** The sender picks the type. A blob: URL shares our origin, so only inert media types
 *  keep theirs; HTML, SVG and the rest open as plain bytes and can only be downloaded. */
const safeMime = (mime: string) => /^(image\/(png|jpeg|gif|webp|avif|heic)|video\/(mp4|webm|quicktime)|audio\/(webm|mp4|mpeg|ogg|aac|wav))$/.test(mime) ? mime : 'application/octet-stream';

/** No session-long cache of decrypted bytes: URLs live only as long as their consumer. */
function openUrl(messages: Messages, file: FileDoc, thumbnail = false) {
  return acquireUrl(() => messages.openFile(file, thumbnail), safeMime);
}
function useOpened(messages: Messages | null, file: FileDoc | null, thumbnail = false, enabled = true) {
  const key = JSON.stringify([file, thumbnail]);
  const [state, setState] = useState<{ key?: string; url?: string; error?: string }>({});
  useEffect(() => {
    if (!messages || !file || !enabled) return;
    let live = true;
    const opened = openUrl(messages, file, thumbnail);
    opened.url.then((url) => live && setState({ key, url })).catch((e: unknown) => live && setState({ key, error: reason(e) }));
    return () => { live = false; opened.release(); };
  }, [messages, key, enabled]);
  return state.key === key ? state : {};
}

export const kindOf = (mime: string) => mime.startsWith('image/') ? 'image' : mime.startsWith('video/') ? 'video' : mime.startsWith('audio/') ? 'audio' : 'file';
const size = (n: number) => n < 1024 ? `${n} B` : n < 1 << 20 ? `${Math.round(n / 1024)} KB` : `${(n / (1 << 20)).toFixed(1).replace('.', ',')} MB`;
const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;

export function FileView({ file, messages, presentation }: { file: FileDoc; messages: Messages | null; presentation?: 'media' | 'file' | undefined }) {
  const kind = presentation === 'file' ? 'file' : kindOf(file.mime);
  if (kind === 'audio') return <VoicePlayer file={file} messages={messages} />;
  if (kind === 'image' || kind === 'video') return <Visual file={file} messages={messages} kind={kind} />;
  return <FileCard file={file} messages={messages} />;
}

function Visual({ file, messages, kind }: { file: FileDoc; messages: Messages | null; kind: 'image' | 'video' }) {
  const [full, setFull] = useState(false);
  const small = kind === 'image' && file.bytes <= 2 << 20;
  const thumb = useOpened(messages, file.thumb || small ? file : null, !!file.thumb);
  const whole = useOpened(messages, file, false, full || (small && !file.thumb));
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
      const opened = openUrl(messages, file);
      void opened.url.then((url) => { const a = document.createElement('a'); a.href = url; a.download = file.name; a.click(); })
        .catch((e: unknown) => setError(reason(e))).finally(() => { setTimeout(opened.release, 0); setBusy(false); });
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
  const opened = useOpened(messages, file);
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
  let videoUrl: string | undefined;
  let bitmap: ImageBitmap | undefined;
  try {
    let source: CanvasImageSource; let w: number; let h: number;
    if (file.type.startsWith('image/')) { const b = await createImageBitmap(file); bitmap = b; source = b; w = b.width; h = b.height; }
    else if (file.type.startsWith('video/')) {
      const v = document.createElement('video'); v.muted = true; v.playsInline = true; v.src = videoUrl = URL.createObjectURL(file);
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
  finally { if (videoUrl) URL.revokeObjectURL(videoUrl); bitmap?.close(); }
}

/** Hold-free voice recording (tap mic, tap send), with live levels. */
export function useRecorder(done: (clip: { bytes: Uint8Array; mime: string; seconds: number; levels: string }) => void, fail: (why: string) => void) {
  const [recording, setRecording] = useState<{ started: number; levels: number[] } | null>(null);
  type Parts = { recorder: MediaRecorder | null; stream: MediaStream | null; chunks: Blob[]; keep: boolean; raf: number; context: AudioContext | null; levels: number[]; stopped: number };
  const parts = useRef<Parts | null>(null);
  const mounted = useRef(true);
  const [now, setNow] = useState(0);
  const live = recording !== null;
  function release(p: Parts) {
    cancelAnimationFrame(p.raf);
    p.stream?.getTracks().forEach((t) => t.stop()); p.stream = null;
    if (p.context) { void p.context.close().catch(() => {}); p.context = null; }
  }
  function discard() {
    const p = parts.current; parts.current = null;
    if (!p) return;
    p.keep = false; p.chunks.length = 0;
    if (p.recorder) {
      p.recorder.ondataavailable = null; p.recorder.onstop = null; p.recorder.onerror = null;
      try { if (p.recorder.state !== 'inactive') p.recorder.stop(); }
      finally { release(p); }
    } else release(p);
  }
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; discard(); }; }, []);
  useEffect(() => { if (!live) return; const t = setInterval(() => setNow(Date.now()), 250); return () => clearInterval(t); }, [live]);
  async function start() {
    if (!mounted.current || parts.current) return;
    const state: Parts = { recorder: null, stream: null, chunks: [], keep: false, raf: 0, context: null, levels: [], stopped: 0 };
    parts.current = state;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (parts.current !== state) { stream.getTracks().forEach((t) => t.stop()); return; }
      state.stream = stream;
      const mime = ['audio/mp4;codecs=mp4a.40.2', 'audio/mp4', 'audio/webm;codecs=opus', 'audio/webm'].find((m) => MediaRecorder.isTypeSupported(m)) ?? '';
      const recorder = new MediaRecorder(stream, mime ? { mimeType: mime } : {});
      state.recorder = recorder;
      const context = new AudioContext(); state.context = context;
      void context.resume().catch(failed);
      const analyser = context.createAnalyser(); analyser.fftSize = 256;
      context.createMediaStreamSource(stream).connect(analyser);
      const buffer = new Uint8Array(analyser.frequencyBinCount);
      const started = Date.now();
      const tick = () => { analyser.getByteTimeDomainData(buffer); let m = 0; for (const v of buffer) m = Math.max(m, Math.abs(v - 128) / 128);
        const peak = Math.min(1, m * 2.4); state.levels.push(peak);
        setRecording((r) => r && { ...r, levels: [...r.levels.slice(-27), Math.max(0.08, peak)] }); state.raf = requestAnimationFrame(tick); };
      recorder.ondataavailable = (e) => { if (e.data.size) state.chunks.push(e.data); };
      recorder.onstop = () => {
        release(state); setRecording(null);
        if (!state.keep) { discard(); return; }
        const blob = new Blob(state.chunks, { type: recorder.mimeType || 'audio/webm' });
        state.chunks.length = 0;
        // Keep the whole recording in at most 160 hex bars, like VoiceMeta::encode.
        const count = Math.min(160, state.levels.length);
        const levels = Array.from({ length: count }, (_, i) => {
          let peak = 0;
          for (let j = Math.floor(i * state.levels.length / count); j < Math.floor((i + 1) * state.levels.length / count); j++) peak = Math.max(peak, state.levels[j]!);
          return Math.round(peak * 15).toString(16);
        }).join('');
        void blob.arrayBuffer().then((b) => {
          if (parts.current !== state) return;
          discard();
          done({ bytes: new Uint8Array(b), mime: (recorder.mimeType || 'audio/webm').split(';')[0]!, seconds: (state.stopped - started) / 1000, levels });
        }).catch(() => failed());
      };
      function failed() {
        if (parts.current !== state) return;
        discard(); setRecording(null); fail('Não deu para gravar. Tente de novo.');
      }
      recorder.onerror = failed;
      recorder.start(250); setRecording({ started, levels: [] }); setNow(started); tick();
    } catch {
      if (parts.current !== state) return;
      discard(); setRecording(null); fail('Libere o microfone no navegador para gravar.');
    }
  }
  function stop(keep: boolean) {
    const p = parts.current; if (!p) return;
    if (keep && p.keep) return;
    if (!keep || !p.recorder || p.recorder.state === 'inactive') { discard(); setRecording(null); return; }
    p.keep = true; p.stopped = Date.now(); p.recorder.stop(); release(p); setRecording(null);
  }
  return { recording, seconds: recording ? (now - recording.started) / 1000 : 0, start, send: () => stop(true), cancel: () => stop(false), clock };
}
