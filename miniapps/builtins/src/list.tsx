import { useState } from 'react';
import { Pill } from '@zoen/ui';
import {
  Activity,
  Empty,
  Refresh,
  Section,
  Shell,
  useBuiltin,
  zoen,
  type LogEntry,
} from './shared';
type ListState = {
  title: string;
  items: { id: string; text: string; done: boolean; by: string | null }[];
  log: LogEntry[];
};
export function List() {
  const { state, busy, error, L, call, run } = useBuiltin<ListState>();
  const [text, setText] = useState('');
  if (!state)
    return (
      <Shell title={L('Lista do grupo', 'Group list')}>
        <Empty>{L('Abrindo a lista compartilhada…', 'Opening the shared list…')}</Empty>
      </Shell>
    );
  const items = state.items,
    done = items.filter((item) => item.done).length;
  const share = () => {
    const current = zoen.state.get<ListState>();
    return `${current.title}\n${current.items.map((item) => `${item.done ? '☑' : '☐'} ${item.text}`).join('\n')}`;
  };
  return (
    <Shell
      title={state.title}
      subtitle={L(`${done} de ${items.length} concluídos`, `${done} of ${items.length} complete`)}
      busy={busy}
      error={error}
    >
      <Section>
        <ul className="list">
          {items.map((item) => (
            <li key={item.id}>
              <label className="item" data-checked={item.done}>
                <input
                  type="checkbox"
                  aria-checked={item.done}
                  checked={item.done}
                  disabled={busy}
                  onChange={() => void call('list_toggle', { id: item.id })}
                />
                <span className="grow">
                  <span className="item-text">{item.text}</span>
                  {item.by && <small className="muted"> · {item.by}</small>}
                </span>
              </label>
            </li>
          ))}
        </ul>
        {!items.length && (
          <Empty>
            {L('Adicione a primeira coisa para levar.', 'Add the first thing to bring.')}
          </Empty>
        )}
      </Section>
      <Section title={L('Adicionar item', 'Add an item')}>
        <form
          className="form"
          onSubmit={(event) => {
            event.preventDefault();
            const captured = text.trim();
            if (captured)
              void call('list_add', { text: captured }).then((ok) => {
                if (ok) setText((current) => (current.trim() === captured ? '' : current));
              });
          }}
        >
          <label className="field">
            {L('O que levar?', 'What to bring?')}
            <input
              value={text}
              disabled={busy}
              maxLength={1000}
              onChange={(event) => setText(event.target.value)}
            />
          </label>
          <Pill disabled={busy || !text.trim()} onClick={undefined}>
            {L('Adicionar', 'Add')}
          </Pill>
        </form>
      </Section>
      <Section title={L('Compartilhar', 'Share')}>
        <div className="row">
          <Pill disabled={busy} onClick={() => void run(() => zoen.share(share()))}>
            {L('Na conversa', 'In this chat')}
          </Pill>
          <Pill
            disabled={busy}
            tone="glass"
            onClick={() =>
              void run(() => zoen.openLink(`https://wa.me/?text=${encodeURIComponent(share())}`))
            }
          >
            {L('Abrir no WhatsApp', 'Open in WhatsApp')}
          </Pill>
        </div>
        <p className="muted">
          {L(
            'Você confirma antes de compartilhar. Abrir o WhatsApp não confirma o envio.',
            'You confirm before sharing. Opening WhatsApp does not confirm delivery.',
          )}
        </p>
      </Section>
      <Activity entries={state.log} L={L} />
      <Refresh tool="list_get_state" busy={busy} onCall={call} L={L} />
    </Shell>
  );
}
