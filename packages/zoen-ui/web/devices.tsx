import { useEffect, useState } from 'react';
import { array, boolean, object, publicKey, string } from './boundary.ts';
import type { Messages } from './messages.ts';

export function parseDevices(value: unknown) {
  const response = object(value);
  if (string(response.kind) !== 'devices') throw new Error('Resposta inesperada.');
  return array(response.devices).map((value) => {
    const device = object(value);
    return { device: publicKey(device.device), revoked: boolean(device.revoked) };
  });
}

export function Devices({ messages, current, close }: { messages: Messages | null; current: string; close: () => void }) {
  const [devices, setDevices] = useState<ReturnType<typeof parseDevices> | null>(null);
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState('');
  useEffect(() => {
    let stopped = false;
    setBusy(true);
    if (!messages) { setBusy(false); setError('Aguarde a conexão e tente de novo.'); return; }
    void messages.devices().then((value) => {
      if (!stopped) { setDevices(parseDevices(value)); setError(''); }
    }).catch(() => { if (!stopped) setError('Não foi possível atualizar os aparelhos. Tente de novo.'); })
      .finally(() => { if (!stopped) setBusy(false); });
    return () => { stopped = true; };
  }, [messages, revision]);
  return <section className="zw-gate" aria-label="Aparelhos">
    <h2>Aparelhos</h2>
    <p>Estes aparelhos podem receber suas mensagens.</p>
    {devices?.filter((device) => !device.revoked).map(({ device }) => <div key={device}>
      <p>{device === current ? 'Este navegador' : 'Outro aparelho'} · {device.slice(0, 8)}</p>
      {device !== current ? <button disabled={busy} onClick={() => setConfirm(device)}>Desconectar aparelho</button> : null}
    </div>)}
    <p>Para conectar outro aparelho, abra o Zoen nele e copie o código. No seu celular, abra Aparelhos e escolha Conectar aparelho.</p>
    {confirm ? <div role="alertdialog" aria-label="Desconectar aparelho?">
      <p>Desconectar o aparelho {confirm.slice(0, 8)}? Ele deixará de receber suas mensagens.</p>
      <button disabled={busy} onClick={() => setConfirm('')}>Cancelar</button>
      <button disabled={busy || !messages} onClick={() => {
        if (!messages) return;
        setBusy(true);
        void messages.unlinkDevice(confirm).then(() => { setConfirm(''); setRevision((value) => value + 1); })
          .catch(() => setError('Não foi possível desconectar. Tente de novo.')).finally(() => setBusy(false));
      }}>Desconectar</button>
    </div> : null}
    {busy ? <p role="status">Atualizando…</p> : null}
    {error ? <p role="alert">{error}</p> : null}
    <button disabled={busy} onClick={() => setRevision((value) => value + 1)}>Atualizar aparelhos</button>
    <button onClick={close}>Voltar às conversas</button>
  </section>;
}
