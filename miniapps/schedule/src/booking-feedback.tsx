import { Pill } from '@zoen/ui';
import { formatMoney, formatTime, type ScheduleState, type Service } from './slots';
import { L } from './i18n';

export function BookingFeedback({ confirm, sent, service, pick, when, currency, locale, tz, accent, onRequestConfirm, requestBook, onDismissConfirm, onDismissSent }: {
  confirm: boolean; sent: boolean; service: Service | undefined;
  pick: { startMs: number; endMs: number } | null; when: (ms: number) => string;
  currency: ScheduleState['currency']; locale: string; tz: string; accent: string; onRequestConfirm: () => void;
  requestBook: () => void; onDismissConfirm: () => void; onDismissSent: () => void;
}) {
  return <>
      {pick && service && (
        <div className="sticky z-glass">
          <div style={{ padding: '0 8px', minWidth: 0 }}>
            <b style={{ display: 'block', fontSize: 14 }}>{service.name}</b>
            <span style={{ fontSize: 13, color: 'var(--z-ink-2)' }}>
              {formatTime(pick.startMs, tz, locale)} ·{' '}
              {formatMoney(service.priceCents, currency, locale)}
            </span>
          </div>
          <Pill
            tone="brand"
            style={{ background: accent }}
            onClick={onRequestConfirm}
          >
            {L('Send request')}
          </Pill>
        </div>
      )}

      {confirm && service && pick && (
        <div
          className="sheet"
          role="dialog"
          aria-label={L('Confirm')}
          onClick={() => onDismissConfirm()}
        >
          <div className="sheet-card" onClick={(e) => e.stopPropagation()}>
            <h2>{L('Confirm')}</h2>
            <div className="sheet-row">
              <span>{L('What')}</span>
              <b>{service.name}</b>
            </div>
            <div className="sheet-row">
              <span>{L('When')}</span>
              <b>{when(pick.startMs)}</b>
            </div>
            <div className="sheet-row">
              <span>{L('Price')}</span>
              <b>{formatMoney(service.priceCents, currency, locale)}</b>
            </div>
            <div className="sheet-row">
              <span>{L('Duration')}</span>
              <b>
                {service.minutes} {L('min')}
              </b>
            </div>
            <p style={{ margin: 0, color: 'var(--z-ink-2)', fontSize: 14 }}>
              {L('Waiting for confirmation')}
            </p>
            <div className="sheet-actions">
              <Pill tone="glass" onClick={() => onDismissConfirm()}>
                {L('Back')}
              </Pill>
              <Pill tone="brand" style={{ background: accent }} onClick={requestBook}>
                {L('Send request')}
              </Pill>
            </div>
          </div>
        </div>
      )}

      {sent && (
        <div className="success" role="status">
          <div className="success-card">
            <div className="stamp" aria-hidden="true">
              <svg width="32" height="32" viewBox="0 0 32 32">
                <path
                  d="M7 17l6 6 12-14"
                  stroke="currentColor"
                  strokeWidth="3.2"
                  fill="none"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </div>
            <h2>{L('Ask sent')}</h2>
            <p>{L('You can add it to your calendar once it’s confirmed')}</p>
            <div className="sheet-actions">
              <Pill tone="brand" style={{ background: accent }} onClick={() => onDismissSent()}>
                {L('Close')}
              </Pill>
            </div>
          </div>
        </div>
      )}

  </>;
}
