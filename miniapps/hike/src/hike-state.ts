import { useCallback, useState } from 'react';
import { zoen, useZoenState, useMembers } from '@zoen/ui';
import type { Member } from '@zoen/miniapp-sdk';
import type { TrailId } from './trails';

export interface HikeState {
  title: string; area: string; day: string; dayMs?: number;
  trails: { id: TrailId; votes: string[] }[];
  decided: TrailId | null;
  itinerary: { time: string; text: string }[] | null;
  album: { by: string; dataUrl: string }[];
}

// Outside a host (a browser tab, the headless test) the app runs on this local demo state.
const DEMO: HikeState = {
  title: 'Saturday hike', area: 'Bay Area', day: 'Saturday',
  trails: [{ id: 'tomales', votes: ['Marina'] }, { id: 'steep', votes: ['Lucas'] }, { id: 'lands', votes: [] }],
  decided: null, itinerary: null, album: [],
};
const DEMO_MEMBERS: Member[] = [
  { id: 'me', name: 'Enzo', initials: 'E', color: '#4F7BEF', isMe: true },
  { id: 'm', name: 'Marina', initials: 'M', color: '#E0567A', isMe: false },
  { id: 'l', name: 'Lucas', initials: 'L', color: '#2FA37C', isMe: false },
  { id: 'a', name: 'Ana', initials: 'A', color: '#F0A030', isMe: false },
];

export function useHike() {
  const live = useZoenState<HikeState>();
  const [local, setLocal] = useState<HikeState>(DEMO);
  const hosted = zoen.capabilities.host !== 'none' && zoen.capabilities.host !== 'unknown';
  const state = (hosted && live?.trails ? live : local) as HikeState;
  const membersLive = useMembers();
  const members = membersLive.length ? membersLive : DEMO_MEMBERS;
  const me = members.find((m) => m.isMe) ?? members[0];
  const call = useCallback(async (tool: string, args: Record<string, any> = {}) => {
    if (hosted) return zoen.state.call(tool, args);
    // Local demo: mimic the core.
    setLocal((s) => {
      const n: HikeState = JSON.parse(JSON.stringify(s));
      if (tool === 'hike_vote') { n.trails.forEach((t) => (t.votes = t.votes.filter((v) => v !== me.name))); n.trails.find((t) => t.id === args.trail)!.votes.push(me.name); }
      if (tool === 'hike_decide') { n.decided = [...n.trails].sort((a, b) => b.votes.length - a.votes.length)[0].id; }
      return n;
    });
  }, [hosted, me?.name]);
  return { state, members, me, call, hosted };
}

export function votersOf(state: HikeState, id: TrailId, members: Member[]) {
  const names = state.trails.find((t) => t.id === id)?.votes ?? [];
  return names.map((n) => members.find((m) => m.name === n) ?? { id: n, name: n, initials: n.slice(0, 1).toUpperCase(), color: '#8C95A8', isMe: false });
}

