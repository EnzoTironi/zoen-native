import { useEffect, useId, useRef, useState } from 'react';
import { ShellIcon, ShellPinnedCard, ShellPinnedCards } from '@zoen/ui/shell';

const planLines = [
  ['Bus Rio → Paraty, round trip for two', '$248'],
  ['Taxi between the bus station and inn', '$60'],
  ['Casa Azul Inn, two nights', '$640'],
  ['Schooner around the islands', '$180'],
  ['Praia do Sono trail', 'Free'],
  ['Dinner at Banana da Terra', '$260'],
  ['Packing list', 'Free'],
] satisfies readonly (readonly [string, string])[];

export function SampleChatPins() {
  const [open, setOpen] = useState<'plan' | 'countdown' | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const headingId = useId();
  useEffect(() => {
    if (open) dialog.current?.showModal();
    else dialog.current?.close();
  }, [open]);
  return (
    <>
      <ShellPinnedCards>
        <ShellPinnedCard title="Weekend in Paraty" summary="7 items · $1,388"
          art={<ShellIcon name="files" />} onOpen={() => setOpen('plan')} />
        <ShellPinnedCard title="Paraty" summary="9 days to go"
          art={<ShellIcon name="sun" />} onOpen={() => setOpen('countdown')} />
      </ShellPinnedCards>
      <dialog ref={dialog} className="preview-pin-detail" aria-labelledby={headingId}
        onClose={() => setOpen(null)}>
        <form method="dialog"><button type="submit" aria-label="Close card"><ShellIcon name="close" /></button></form>
        <span className="preview-eyebrow">Sample card · Local preview</span>
        <h2 id={headingId}>{open === 'plan' ? 'Weekend in Paraty' : 'Paraty countdown'}</h2>
        {open === 'plan' ? <>
          <p>Friday to Sunday · Enzo and Marina</p>
          <p><strong>$1,388</strong> of $1,500 · $112 left</p>
          <ul>{planLines.map(([title, price]) => <li key={title}><span>{title}</span><strong>{price}</strong></li>)}</ul>
        </> : <p>9 days to go. This sample date stays fixed for layout review.</p>}
      </dialog>
    </>
  );
}
