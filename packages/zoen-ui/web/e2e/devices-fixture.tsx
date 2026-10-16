// Visual fixture only. No relay, enrollment, identity material or real revocation.
import { createRoot } from 'react-dom/client';
import { Devices } from '../devices.tsx';
import type { Messages } from '../messages.ts';
import '../web.css';

const current = '11'.repeat(32);
const other = '22'.repeat(32);
let revoked = false;
let fail = false;
const messages = {
  devices: async () => {
    if (fail) { fail = false; throw new Error('fixture offline'); }
    return { kind: 'devices', devices: [{ device: current, revoked: false }, { device: other, revoked }] };
  },
  unlinkDevice: async (device: string) => { if (device !== other) throw new Error('fixture target'); revoked = true; },
} as unknown as Messages;
createRoot(document.getElementById('root')!).render(<>
  <p>Fixture visual — dados inventados, sem conexão a aparelhos reais.</p>
  <button onClick={() => { fail = true; }}>Simular falha na próxima atualização</button>
  <Devices messages={messages} current={current} close={() => {}} />
</>);
