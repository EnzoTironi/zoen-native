import { type LiveActivityState } from './live-activity.ts';
import type { Mode, Snapshot, TimelinePage } from './types';

export type ClientState = {
  phase: 'opening' | 'erasing' | 'onboarding' | 'ready' | 'failed';
  erasePending: boolean;
  mode: Mode;
  snapshot: Snapshot | null;
  timelines: Readonly<Record<string, TimelinePage>>;
  notice: string | null;
  activity: LiveActivityState;
};

export type SessionRetirement = {
  task: Promise<void>;
  cancelled: boolean;
  force: boolean;
  nativeCloseStarted: boolean;
  forceTask?: Promise<void>;
};
