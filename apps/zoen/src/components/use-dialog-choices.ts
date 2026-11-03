import { useEffect, useState, useSyncExternalStore } from 'react';
import { DialogChoiceQueue } from './dialog-choice-queue';

export function useDialogChoices() {
  const [queue] = useState(() => new DialogChoiceQueue());
  const pending = useSyncExternalStore(queue.subscribe, queue.getSnapshot, () => null);
  useEffect(() => {
    queue.activate();
    return () => queue.retire();
  }, [queue]);
  return { pending, choose: queue.choose, finish: queue.finish };
}
