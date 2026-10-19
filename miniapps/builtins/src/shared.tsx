import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { zoen, useZoenState, useTheme, Pill } from '@zoen/ui';
import type { Json } from '@zoen/miniapp-sdk';
import '@zoen/ui/tokens.css';
import './styles.css';

export { zoen };
/** Sandboxed MCP views cannot submit HTML forms, including React onSubmit. */
export function ActionForm({ children, onSubmit }: { children: ReactNode; onSubmit: () => void }) {
  return (
    <div
      className="form"
      onKeyDown={(event) => {
        if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
          event.preventDefault();
          onSubmit();
        }
      }}
    >
      {children}
    </div>
  );
}
export function useBuiltin<T>() {
  const state = useZoenState<T>();
  useTheme();
  const [ready, setReady] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
  const current = useRef(false),
    pending = useRef(false);
  useLayoutEffect(() => {
    current.current = true;
    return () => {
      current.current = false;
    };
  }, []);
  useEffect(() => {
    void zoen.ready().then(() => {
      if (current.current) setReady(true);
    });
  }, []);
  const L = (pt: string, en: string) => (zoen.locale.toLowerCase().startsWith('pt') ? pt : en);
  async function run(work: () => Promise<unknown>): Promise<boolean> {
    if (!current.current || pending.current || !state) return false;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      await work();
      return current.current;
    } catch (reason) {
      if (current.current)
        setError(
          reason instanceof Error
            ? reason.message
            : L('Não foi possível concluir. Tente novamente.', 'Could not finish. Try again.'),
        );
      return false;
    } finally {
      pending.current = false;
      if (current.current) setBusy(false);
    }
  }
  return {
    state,
    busy,
    error,
    ready,
    L,
    run,
    call: (tool: string, args: Record<string, Json> = {}) => run(() => zoen.state.call(tool, args)),
  };
}
export function Shell({
  title,
  subtitle,
  busy = false,
  error,
  children,
}: {
  title: string;
  subtitle?: string;
  busy?: boolean;
  error?: string | null;
  children: ReactNode;
}) {
  return (
    <main className="builtin" aria-busy={busy}>
      <header>
        <h1>{title}</h1>
        {subtitle && <p className="muted">{subtitle}</p>}
      </header>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {children}
    </main>
  );
}
export function Section({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="section">
      {title && <h2>{title}</h2>}
      {children}
    </section>
  );
}
export function Empty({ children }: { children: ReactNode }) {
  return <output className="muted">{children}</output>;
}
export function Refresh({
  tool,
  busy,
  onCall,
  L,
}: {
  tool: string;
  busy: boolean;
  onCall: (tool: string) => Promise<boolean>;
  L: (pt: string, en: string) => string;
}) {
  return (
    <Pill tone="glass" disabled={busy} onClick={() => void onCall(tool)}>
      {L('Atualizar', 'Refresh')}
    </Pill>
  );
}
export type LogEntry = { who: string; what: string; at: number };
export function Activity({
  entries,
  L,
}: {
  entries?: LogEntry[];
  L: (pt: string, en: string) => string;
}) {
  return entries?.length ? (
    <Section title={L('No grupo', 'In the group')}>
      <ul className="activity">
        {entries.slice(0, 5).map((entry, index) => (
          <li key={`${entry.at}:${index}`}>
            <b>{entry.who}</b> {entry.what}
          </li>
        ))}
      </ul>
    </Section>
  ) : null;
}
