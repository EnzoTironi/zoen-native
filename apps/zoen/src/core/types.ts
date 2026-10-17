import type { Command } from './generated/Command';
import type { Snapshot } from './generated/Snapshot';
import type { TimelinePage } from './generated/TimelinePage';
import type { TimelineEntry } from './generated/TimelineEntry';
import type { ItemDetail } from './generated/ItemDetail';
import type { PageDto } from './generated/PageDto';
import type { PlanPageDto } from './generated/PlanPageDto';
import type { SearchHit } from './generated/SearchHit';
import type { AgentProfile } from './generated/AgentProfile';
import type { AgentRequestDto } from './generated/AgentRequestDto';
import type { AppSpecDto } from './generated/AppSpecDto';
import type { AppCallOutcome } from './generated/AppCallOutcome';
import type { DeviceDto } from './generated/DeviceDto';
import type { InviteDto } from './generated/InviteDto';
import type { InvitePreviewDto } from './generated/InvitePreviewDto';
import type { ProfileDto } from './generated/ProfileDto';
import type { StandingDecisionDto } from './generated/StandingDecisionDto';
import type { LogReport } from './generated/LogReport';
import type { LogEventDto } from './generated/LogEventDto';
import type { MessageSearchPage } from './generated/MessageSearchPage';
import type { PageEditResult } from './generated/PageEditResult';
import type { AgentStatus } from './generated/AgentStatus';
import type { UniversalResults } from './generated/UniversalResults';
import type { PageBlockDto } from './generated/PageBlockDto';
import type { MediaDriver } from '../media/types';
import type { EditOutcome } from './generated/EditOutcome';
import type { BackupStatusDto } from './generated/BackupStatusDto';
import type { LinkRequestDto } from './generated/LinkRequestDto';
import type { LinkedDto } from './generated/LinkedDto';
import type { HistoryDto } from './generated/HistoryDto';
import type { HistoryStatusDto } from './generated/HistoryStatusDto';
import type { OlderDto } from './generated/OlderDto';
import type { AccountDto } from './generated/AccountDto';
import type { DecisionPreview } from './generated/DecisionPreview';
import type { AgentActivityDto } from './generated/AgentActivityDto';
import type { Mention } from './generated/Mention';
import type { DeviceGrantDto } from './generated/DeviceGrantDto';
import type { DecideOutcome } from './generated/DecideOutcome';
import type { AppResourceDto } from './generated/AppResourceDto';

export type { Command, Snapshot, TimelinePage, TimelineEntry, ItemDetail, PageDto, PlanPageDto };
export type Mode = 'account' | 'demo';
export type PairingStatus =
  | { kind: 'starting' }
  | { kind: 'waiting'; code: string; check: string }
  | { kind: 'failed'; message: string };
export type CoreEvent = {
  kind: 'change' | 'connection' | 'profile' | 'presence' | 'ephemeral' | 'error';
  spaceIds?: string[];
  spaceId?: string;
  fromId?: string;
  type?: string;
  detail?: string;
  message?: string;
};
type Results = {
  snapshot: Snapshot;
  timeline: TimelinePage;
  send_message: TimelineEntry;
  thread: TimelineEntry[];
  items: ItemDetail[];
  item: ItemDetail;
  item_at: ItemDetail;
  page: PageDto;
  page_at: PageDto;
  plan_page: PlanPageDto;
  page_create: ItemDetail;
  page_import_markdown: ItemDetail;
  page_markdown: string;
  requests: AgentRequestDto[];
  agents: AgentProfile[];
  agent_profile: AgentProfile;
  preview_decisions: DecisionPreview[];
  agent_activity: AgentActivityDto[];
  mentions: Mention[];
  app_device_grants: DeviceGrantDto[];
  app_device_allowed: boolean;
  grant_app_device: string;
  decide_request: DecideOutcome;
  app_resource: AppResourceDto;
  raise_budget: AgentProfile;
  search: SearchHit[];
  find_people: Snapshot['conversations'][number]['members'];
  app_specs: AppSpecDto[];
  app_call_tool: AppCallOutcome;
  install_app: ItemDetail;
  devices: DeviceDto[];
  start_direct: string;
  create_group: string;
  create_community: string;
  create_invite: InviteDto;
  create_agent_invite: InviteDto;
  preview_invite: InvitePreviewDto;
  join_invite: string;
  profile: ProfileDto;
  standing_decisions: StandingDecisionDto[];
  verify_log: LogReport;
  log_events: LogEventDto[];
  search_messages: MessageSearchPage;
  page_apply: PageEditResult;
  agent_status: AgentStatus;
  open_agent_link: string;
  page_commit: boolean;
  blocked_people: string[];
  message: TimelineEntry;
  universal_search: UniversalResults;
  new_mutation_id: string;
  page_parse_markdown: PageBlockDto[];
  page_block_markdown: string;
  toggle_plan_line: EditOutcome;
  edit_plan_line: EditOutcome;
  add_plan_line: EditOutcome;
  remove_plan_line: EditOutcome;
  restore_plan_version: ItemDetail;
  restore_version: ItemDetail;
  undo: ItemDetail;
  backup_status: BackupStatusDto;
  backup_create_recovery_key: string;
  backup_enable_recovery_key: BackupStatusDto;
  backup_password: BackupStatusDto;
  backup_now: BackupStatusDto;
  backup_turn_off: void;
  link_request: LinkRequestDto;
  link_code_check: string;
  link_wait: AccountDto;
  link_device: LinkedDto;
  link_progress: number[];
  send_history: number;
  receive_history: HistoryDto;
  history_status: HistoryStatusDto;
  load_older: OlderDto;
  restore_backup_password: AccountDto;
  restore_backup_recovery_key: AccountDto;
  provider_sign_in: AccountDto;
  update_handle: AccountDto;
};
export type CommandResult<C extends Command> = C['op'] extends keyof Results
  ? Results[C['op']]
  : unknown;

export interface CoreDriver {
  media?: MediaDriver;
  open(mode: Mode): Promise<Snapshot>;
  request<C extends Command>(command: C): Promise<CommandResult<C>>;
  createAccount(name: string, handle: string, relay: string): Promise<Snapshot>;
  subscribe(listener: (event: CoreEvent) => void): () => void;
  close(): Promise<void>;
  pair?(
    relay: string,
    changed: (state: PairingStatus) => void,
    expectedIdentityId?: string,
  ): () => void;
}
