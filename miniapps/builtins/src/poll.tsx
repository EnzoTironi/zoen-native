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
type PollState = {
  question: string;
  options: { id: string; text: string; votes: string[] }[];
  closed: boolean;
  winner: string | null;
  log: LogEntry[];
};
export function Poll() {
  const { state, busy, error, L, call } = useBuiltin<PollState>();
  const [text, setText] = useState('');
  if (!state)
    return (
      <Shell title={L('Enquete', 'Poll')}>
        <Empty>{L('Abrindo a votação…', 'Opening the poll…')}</Empty>
      </Shell>
    );
  const total = state.options.reduce((sum, option) => sum + option.votes.length, 0);
  const myName = zoen.members.me()?.name;
  return (
    <Shell
      title={state.question}
      subtitle={
        state.closed
          ? L('Votação encerrada', 'Voting closed')
          : L(
              'Uma escolha por pessoa. Você pode trocar seu voto.',
              'One choice per person. You can change your vote.',
            )
      }
      busy={busy}
      error={error}
    >
      {state.closed && (
        <Section title={L('Escolha do grupo', 'Group choice')}>
          <p className="hero">{state.winner}</p>
        </Section>
      )}
      <Section>
        <ul className="list">
          {state.options.map((option) => (
            <li key={option.id}>
              <button
                className="poll-option"
                disabled={busy || state.closed}
                aria-pressed={!!myName && option.votes.includes(myName)}
                onClick={() => void call('poll_vote', { option: option.id })}
              >
                <span className="row spread">
                  <b>{option.text}</b>
                  <span>
                    {option.votes.length} {L('votos', 'votes')}
                  </span>
                </span>
                <progress
                  max={Math.max(1, total)}
                  value={option.votes.length}
                  aria-label={option.text}
                />
                {option.votes.length > 0 && (
                  <small className="muted">{option.votes.join(', ')}</small>
                )}
              </button>
            </li>
          ))}
        </ul>
      </Section>
      {!state.closed && (
        <>
          <Section title={L('Sugerir outra opção', 'Suggest another option')}>
            <form
              className="form"
              onSubmit={(event) => {
                event.preventDefault();
                const captured = text.trim();
                if (captured)
                  void call('poll_add_option', { text: captured }).then((ok) => {
                    if (ok) setText((current) => (current.trim() === captured ? '' : current));
                  });
              }}
            >
              <label className="field">
                {L('Opção', 'Option')}
                <input
                  value={text}
                  disabled={busy}
                  maxLength={1000}
                  onChange={(event) => setText(event.target.value)}
                />
              </label>
              <Pill disabled={busy || !text.trim()}>{L('Adicionar', 'Add')}</Pill>
            </form>
          </Section>
          <Pill disabled={busy || total === 0} onClick={() => void call('poll_close')}>
            {L('Fechar e levar ao plano', 'Close and add to plan')}
          </Pill>
        </>
      )}
      <Activity entries={state.log} L={L} />
      <Refresh tool="poll_get_state" busy={busy} onCall={call} L={L} />
    </Shell>
  );
}
