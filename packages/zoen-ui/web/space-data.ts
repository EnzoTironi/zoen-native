import { array, boolean, integer, object, string } from './boundary.ts';
import type { HackathonState, CommunityTier, CommunityMember } from './hackathon.tsx';
const strings = (input: unknown) => array(input).map(string);
const nullable = (input: unknown) => input === null ? null : string(input);
export interface SpaceItem { id: string; title: string; kind: string; text: string }
export interface SpaceHome {
  page: string | null; coverFile: string | null; canManage: boolean; conversationSpace: string | null;
  pendingRequests: string[]; invitations: { id: string; tier_id: string; expires_at_ms: number; revoked: boolean }[];
  availablePages: { id: string; title: string }[];
  announcements: SpaceItem[]; files: { id: string; title: string; mime: string; bytes: number }[];
  events: { id: string; title: string; stateJson: string }[];
  hackathon: HackathonState | null; canJudge: boolean;
  teamRequests: { team_id: string; identity_id: string }[];
  tiers: CommunityTier[]; tierClaims: { identity_id: string; invite_id: string; tier_id: string }[];
  members: CommunityMember[];
}
/** Strict DTO boundary. Redacted partition IDs stay null; the UI cannot turn labels into access. */
export function parseSpaceHome(input: unknown): SpaceHome {
  const value = object(input);
  const h = value.hackathon === null ? null : object(value.hackathon);
  return {
    page: nullable(value.page), coverFile: nullable(value.cover_file), canManage: boolean(value.can_manage), conversationSpace: nullable(value.conversation_space), pendingRequests: strings(value.pending_requests),
    invitations: value.invitations === undefined ? [] : array(value.invitations).map((raw) => { const i = object(raw); return { id: string(i.id), tier_id: string(i.tier_id), expires_at_ms: integer(i.expires_at_ms), revoked: boolean(i.revoked) }; }),
    availablePages: value.page_details === undefined ? [] : array(value.page_details).map((raw) => { const p = object(raw); return { id: string(p.id), title: string(p.title) }; }),
    announcements: array(value.announcement_details).map((raw) => { const a = object(raw); return { id: string(a.id), title: string(a.title), kind: string(a.kind), text: string(a.text) }; }),
    files: array(value.file_details).map((raw) => { const f = object(raw); return { id: string(f.id), title: string(f.title), mime: string(f.mime), bytes: integer(f.bytes) }; }),
    events: array(value.event_details).map((raw) => { const e = object(raw); return { id: string(e.id), title: string(e.title), stateJson: string(e.state_json) }; }),
    hackathon: h ? { id: string(h.id), title: string(h.title), deadline_ms: integer(h.deadline_ms), registered: strings(h.registered), profiles: Object.fromEntries(Object.entries(object(h.profiles)).map(([id, profile]) => [id, string(profile)])),
      teams: array(h.teams).map((raw) => { const t = object(raw); return { id: string(t.id), name: string(t.name), space: nullable(t.space), members: strings(t.members), project_url: nullable(t.project_url), files: strings(t.files), votes: integer(t.votes), score: integer(t.score) }; }),
      schedule: array(h.schedule).map((raw) => { const s = object(raw); return { title: string(s.title), at_ms: integer(s.at_ms) }; }), winners: strings(h.winners), finalized: boolean(h.finalized) } : null,
    canJudge: h ? boolean(h.can_judge) : false,
    teamRequests: h ? array(h.pending_team_requests).map((raw) => { const r = object(raw); return { team_id: string(r.team_id), identity_id: string(r.identity_id) }; }) : [],
    tiers: array(value.tiers).map((raw) => { const t = object(raw); return { id: string(t.id), name: string(t.name), space: nullable(t.space), groups: strings(t.groups), resources: array(t.resources).map((raw) => { const r = object(raw); const kind = string(r.kind); if (kind !== 'page' && kind !== 'event') throw Error('Invalid membership resource.'); return { id: string(r.id), kind }; }), members: strings(t.members) }; }),
    tierClaims: array(value.tier_claims).map((raw) => { const c = object(raw); return { identity_id: string(c.identity_id), invite_id: string(c.invite_id), tier_id: string(c.tier_id) }; }),
    members: value.member_details === undefined ? [] : array(value.member_details).map((raw) => { const m = object(raw); return { id: string(m.id), name: string(m.name) }; }),
  };
}
