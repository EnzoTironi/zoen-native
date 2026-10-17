import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { JSDOM } from 'jsdom';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
const output = resolve(import.meta.dirname, '../node_modules/.cache/hackathon-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'hackathon.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic' });
const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost/' });
Object.assign(globalThis, { window: dom.window, document: dom.window.document, HTMLElement: dom.window.HTMLElement, FormData: dom.window.FormData, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: dom.window.navigator });
const { createElement: h, act } = await import('react');
const { createRoot } = await import('react-dom/client');
const { Hackathon, CommunityTiers, countdown, validProjectUrl, tierInviteToken } = await import(pathToFileURL(output).href);
test('redacted tier space prevents conversation and resource grants', async () => {
  const calls: unknown[] = [];
  await mounted(CommunityTiers, { tiers: [{ id: 'tier', name: 'Founders', space: null, groups: [], resources: [], members: [] }], identityId: 'me', isAdmin: true, currentSpace: 'home', command: async (c: unknown) => { calls.push(c); } }, async () => {
    for (const label of ['Create membership conversation', 'Unlock for membership']) {
      const button = [...document.querySelectorAll('button')].find(b => b.textContent === label)!;
      assert.equal(button.disabled, true);
      await act(async () => { button.closest('form')!.dispatchEvent(new dom.window.Event('submit', { bubbles: true, cancelable: true })); });
    }
    assert.deepEqual(calls, []);
  });
});
const state = () => ({ id: 'h', title: 'Build weekend', deadline_ms: Date.now() + 86400000, registered: ['me'], teams: [{ id: 't', name: 'Builders', space: 'conversation-1', members: ['me'], project_url: null, files: ['existing'], votes: 2, score: 80 }], schedule: [], winners: [], finalized: false });
async function click(label: string) { const button = [...document.querySelectorAll('button')].find(b => b.textContent === label); assert.ok(button, label); await act(async () => button.click()); }
async function input(label: string, value: string) { const field = [...document.querySelectorAll('label')].find(l => l.textContent === label)?.querySelector('input'); assert.ok(field, label); await act(async () => { Object.getOwnPropertyDescriptor(dom.window.HTMLInputElement.prototype, 'value')!.set!.call(field, value); field.dispatchEvent(new dom.window.Event('input', { bubbles: true })); }); }
async function mounted(component: unknown, props: object, run: () => Promise<void>) { const root = createRoot(document.getElementById('root')!); await act(async () => root.render(h(component as any, props))); try { await run(); } finally { await act(async () => root.unmount()); } }
test('member conversation stays disabled when core redacts its space', async () => { let opened = false; await mounted(Hackathon, { hackathon: { ...state(), teams: [{ ...state().teams[0], space: null }] }, identityId: 'me', isAdmin: false, isJudge: false, command: async () => {}, openTeamConversation: async () => { opened = true; } }, async () => { const button = [...document.querySelectorAll('button')].find(b => b.textContent === 'Open conversation'); assert.ok(button); assert.equal(button.disabled, true); await click('Open conversation'); assert.equal(opened, false); }); });
test('admin creates hackathon from actual local deadline and retains rejected drafts', async () => { const calls: unknown[] = []; const props = { hackathon: null, identityId: 'me', isAdmin: true, isJudge: false, command: async (c: unknown) => { calls.push(c); throw Error('Core rejected creation'); }, openTeamConversation: async () => {} }; await mounted(Hackathon, props, async () => { await input('Hackathon title', 'Build day'); await input('Submission deadline', '2027-04-12T18:30'); assert.equal(document.querySelector<HTMLInputElement>('input[type=datetime-local]')!.value, '2027-04-12T18:30'); await click('Create hackathon'); assert.deepEqual(calls, [{ action: 'hackathon_create', title: 'Build day', deadline_ms: new Date('2027-04-12T18:30').getTime() }]); assert.match(document.body.textContent!, /Core rejected creation/); assert.equal(document.querySelector<HTMLInputElement>('input')!.value, 'Build day'); }); await mounted(Hackathon, { ...props, isAdmin: false }, async () => { assert.equal(document.querySelector('form'), null); }); });
test('tier links validate current community and request approval without granting membership', async () => { assert.equal(tierInviteToken('zoen://tier?space=home&token=abc_DEF-123', 'home'), 'abc_DEF-123'); assert.throws(() => tierInviteToken('zoen://tier?space=other&token=abc', 'home'), /different community/); assert.throws(() => tierInviteToken('zoen://tier?space=home&token=a%2Bb', 'home'), /Invalid/); const calls: unknown[] = []; await mounted(CommunityTiers, { tiers: [], identityId: 'me', isAdmin: false, currentSpace: 'home', command: async (c: unknown) => { calls.push(c); } }, async () => { await input('Invitation', 'zoen://tier?space=other&token=abc'); await click('Request access'); assert.equal(calls.length, 0); assert.match(document.body.textContent!, /different community/); await input('Invitation', 'zoen://tier?space=home&token=abc'); await click('Request access'); assert.deepEqual(calls, [{ action: 'tier_redeem', token: 'abc' }]); assert.equal(document.querySelector('.zh-badge'), null); }); });
async function select(label: string, value: string) { const field = [...document.querySelectorAll('label')].find(l => l.firstChild?.textContent === label)?.querySelector('select'); assert.ok(field, label); await act(async () => { field.value = value; field.dispatchEvent(new dom.window.Event('change', { bubbles: true })); }); }
test('named member selectors and authorized pending requests dispatch exact commands', async () => {
  const calls: unknown[] = []; const members = [{ id: 'alice-id', name: 'Alice' }];
  await mounted(Hackathon, { hackathon: state(), identityId: 'me', isAdmin: true, isJudge: false, members, pendingRequests: [{ team_id: 't', identity_id: 'alice-id' }], command: async (c: unknown) => { calls.push(c); }, openTeamConversation: async () => {} }, async () => {
    await select('Judge', 'alice-id'); await click('Assign judge'); await click('Accept Alice');
    assert.deepEqual(calls, [{ action: 'set_judge', identity_id: 'alice-id' }, { action: 'team_join', team_id: 't', identity_id: 'alice-id' }]);
    assert.match(document.body.textContent!, /Alice requests to join Builders/);
  });
  calls.length = 0;
  await mounted(CommunityTiers, { tiers: [{ id: 'tier', name: 'Founders', members: [], space: 'tier-space', groups: [], resources: [] }], identityId: 'me', isAdmin: true, currentSpace: 'home', members, command: async (c: unknown) => { calls.push(c); } }, async () => {
    await select('Member', 'alice-id'); await click('Assign member'); assert.deepEqual(calls, [{ action: 'tier_assign', tier_id: 'tier', identity_id: 'alice-id' }]);
  });
});
test('countdown closes at deadline and URL validation rejects executable links', () => { assert.equal(countdown(1000, 1000), 'Submissions closed'); assert.equal(countdown(1000, 0), '0d 0h 0m 1s remaining'); assert.equal(validProjectUrl('javascript:alert(1)'), false); assert.equal(validProjectUrl('https://example.org'), true); });
test('finder displays actual names and skills only for unteamed registrants', async () => { await mounted(Hackathon, { hackathon: { ...state(), registered: ['me', 'alice'], profiles: { me: 'TEAMED_SKILL', alice: 'Gardening and design' } }, members: [{ id: 'alice', name: 'Alice' }], identityId: 'me', isAdmin: false, isJudge: false, command: async () => {}, openTeamConversation: async () => {} }, async () => { assert.match(document.body.textContent!, /Alice/); assert.match(document.body.textContent!, /Gardening and design/); assert.equal(document.body.textContent!.includes('TEAMED_SKILL'), false); assert.equal(document.body.textContent!.includes('alice'), false); }); });
test('registration calls controller without fabricating registration', async () => { const calls: unknown[] = []; await mounted(Hackathon, { hackathon: { ...state(), registered: [], teams: [] }, identityId: 'me', isAdmin: false, isJudge: false, command: async (c: unknown) => { calls.push(c); }, openTeamConversation: async () => {} }, async () => { await input('What would you like to work on?', 'Design and Swift'); await click('Register'); assert.deepEqual(calls, [{ action: 'register', skills: 'Design and Swift' }]); assert.equal(document.querySelector<HTMLInputElement>('input')!.maxLength, 512); assert.equal(document.body.textContent!.includes('You are registered'), false); }); });
test('conversation callback receives authoritative space and errors remain visible', async () => { const calls: string[] = []; await mounted(Hackathon, { hackathon: state(), identityId: 'me', isAdmin: false, isJudge: false, command: async () => {}, openTeamConversation: async (id: string) => { calls.push(id); throw Error('Transport unavailable'); } }, async () => { await click('Open conversation'); assert.deepEqual(calls, ['conversation-1']); assert.match(document.querySelector('[role=alert]')!.textContent!, /Transport unavailable/); assert.equal(document.body.textContent!.includes('Assign judge'), false); }); });
test('failed submission retains draft and original attachments; authoritative phase freezes form', async () => { const calls: unknown[] = []; const props = { hackathon: state(), identityId: 'me', isAdmin: false, isJudge: false, command: async (c: unknown) => { calls.push(c); throw Error('Rejected by core'); }, openTeamConversation: async () => {} }; await mounted(Hackathon, props, async () => { await input('Project link', 'https://example.org/demo'); await click('Submit project'); assert.deepEqual(calls, [{ action: 'project_submit', team_id: 't', url: 'https://example.org/demo', file_ids: ['existing'] }]); assert.equal(document.querySelector<HTMLInputElement>('input[type=url]')!.value, 'https://example.org/demo'); assert.match(document.body.textContent!, /Rejected by core/); }); await mounted(Hackathon, { ...props, hackathon: { ...state(), deadline_ms: 0, submissions_closed: true } }, async () => { assert.equal(document.querySelector<HTMLInputElement>('input[type=url]')!.disabled, true); assert.match(document.body.textContent!, /Submissions are frozen/); }); });
test('teams, voting and winner announcements remain authoritative', async () => { const calls: unknown[] = []; await mounted(Hackathon, { hackathon: { ...state(), deadline_ms: 0, submissions_closed: true, teams: [{ ...state().teams[0], members: ['other'] }] }, identityId: 'me', isAdmin: true, isJudge: true, command: async (c: unknown) => { calls.push(c); }, openTeamConversation: async () => {} }, async () => { await click('Vote for Builders'); await click('Publish winners announcement'); assert.deepEqual(calls, [{ action: 'vote', team_id: 't' }, { action: 'finalize' }]); assert.match(document.body.textContent!, /Winners will appear/); }); await mounted(Hackathon, { hackathon: { ...state(), finalized: true, winners: ['t'] }, identityId: 'me', isAdmin: false, isJudge: false, command: async () => {}, openTeamConversation: async () => {} }, async () => { assert.equal(document.querySelector('ol li')!.textContent, 'Builders'); }); });
test('tier badges are member-scoped and management is admin-only; failed creation keeps name', async () => { const tiers = [{ id: 'tier', name: 'Founders', members: ['me'], space: 'tier-space', groups: [], resources: [] }]; await mounted(CommunityTiers, { tiers, identityId: 'me', isAdmin: false, command: async () => {} }, async () => { assert.match(document.body.textContent!, /Founders/); assert.equal(document.querySelector('details'), null); }); const calls: unknown[] = []; await mounted(CommunityTiers, { tiers, identityId: 'me', isAdmin: true, command: async (c: unknown) => { calls.push(c); throw Error('Admin denied'); } }, async () => { await input('Membership name', 'Makers'); await click('Create membership'); assert.deepEqual(calls, [{ action: 'tier_create', name: 'Makers' }]); assert.equal(document.querySelector<HTMLInputElement>('input')!.value, 'Makers'); assert.match(document.body.textContent!, /Admin denied/); }); });

test('membership management uses named authorized resource and invitation choices', async () => { const calls: unknown[] = []; await mounted(CommunityTiers, { tiers: [{ id: 't', name: 'Builder', space: 'partition', members: [], groups: [], resources: [] }], identityId: 'me', isAdmin: true, currentSpace: 'home', resources: [{ id: 'page', title: 'Handbook', kind: 'page', space: 'partition' }, { id: 'private', title: 'Other partition', kind: 'page', space: 'elsewhere' }], invitations: [{ id: 'invite', name: 'Builders invitation' }], command: async (c: unknown) => { calls.push(c); } }, async () => { assert.match(document.body.textContent!, /Manage memberships/); assert.equal(document.body.textContent!.includes('Other partition'), false); assert.equal(document.body.textContent!.includes('Restricted resource ID'), false); await select('Unlock resource', 'page'); await click('Unlock for membership'); await select('Invitation to revoke', 'invite'); await click('Revoke invitation'); assert.deepEqual(calls, [{ action: 'tier_bind', tier_id: 't', item_id: 'page', kind: 'page' }, { action: 'tier_invite_revoke', invite_id: 'invite' }]); }); });

test('invitation capabilities stay hidden and copy only on user click', async () => { let copied = ''; Object.defineProperty(dom.window.navigator, 'clipboard', { configurable: true, value: { writeText: async (value: string) => { copied = value; } } }); const capability = ['zoen://tier?space=home', 'token=abc_DEF'].join('&'); await mounted(CommunityTiers, { tiers: [{ id: 't', name: 'Builder', space: 'partition', members: [], groups: [], resources: [] }], identityId: 'me', isAdmin: true, currentSpace: 'home', command: async () => capability }, async () => { assert.equal(document.querySelector<HTMLInputElement>('input[name=token]')!.type, 'password'); await input('Invite expiry', '2027-04-12T18:30'); await click('Create invitation'); assert.equal(document.body.textContent!.includes(capability), false); assert.equal(copied.length, 0); await click('Copy invitation'); assert.ok(copied.length > 0); }); });

test('existing files submit actual selected references without upload', async () => { const calls: any[] = []; await mounted(Hackathon, { hackathon: state(), identityId: 'me', isAdmin: false, isJudge: false, availableFiles: [{id:'file-real',title:'Project notes'}], command: async (c: any) => { calls.push(c); }, openTeamConversation: async () => {} }, async () => { await input('Project link', 'https://example.com/project'); const checkbox = document.querySelector<HTMLInputElement>('input[type=checkbox]')!; await act(async () => checkbox.click()); await click('Submit project'); assert.deepEqual(calls[0].file_ids, ['existing','file-real']); }); });
test('membership content opens only named resolvable authorized references', async () => { const calls: string[] = []; await mounted(CommunityTiers, { tiers: [{id:'tier',name:'Builder',space:'private',members:['me'],groups:['group','unknown-group'],resources:[{id:'page',kind:'page'},{id:'unknown-page',kind:'page'}]}], identityId:'me',isAdmin:false,currentSpace:'community',command:async()=>{},conversations:[{id:'group',title:'Garden builders'}], resources:[{id:'page',title:'Project guide',kind:'page',space:'private'}],openConversation:async(id:string)=>{calls.push(id);},openResource:async(id:string)=>{calls.push(id);} }, async()=>{ await click('Garden builders'); await click('Project guide'); assert.deepEqual(calls,['group','page']); assert.doesNotMatch(document.body.textContent!,/unknown-group|unknown-page/); }); });

test('file-only submissions use existing attachments', async () => { const calls: unknown[] = []; await mounted(Hackathon, {hackathon:state(),identityId:'me',isAdmin:false,isJudge:false,command:async(c:unknown)=>{calls.push(c);},openTeamConversation:async()=>{}}, async()=>{await click('Submit project'); assert.deepEqual(calls,[{action:'project_submit',team_id:'t',url:'',file_ids:['existing']}]);}); });
test('judging and voting require closed phase and other submitted team', async () => { for (const variant of [state(), {...state(),deadline_ms:0}, {...state(),deadline_ms:0,teams:[{...state().teams[0],members:['other'],files:[]}]}, {...state(),deadline_ms:0,finalized:true,teams:[{...state().teams[0],members:['other']}] }]) {await mounted(Hackathon,{hackathon:variant,identityId:'me',isAdmin:true,isJudge:true,command:async()=>{throw Error('Must not dispatch');},openTeamConversation:async()=>{}},async()=>{for(const text of ['Vote for Builders','Submit judge score']) assert.equal([...document.querySelectorAll('button')].find(b=>b.textContent===text)!.disabled,true);assert.equal(document.querySelector<HTMLInputElement>('input[name=score]')!.disabled,true);});} });

test('elapsed schedule cannot open voting without an administrator transition', async () => {
  const calls: unknown[] = [];
  await mounted(Hackathon, { hackathon: { ...state(), deadline_ms: 0, teams: [{ ...state().teams[0], members: ['other'] }] }, identityId: 'me', isAdmin: true, isJudge: true, command: async (c: unknown) => { calls.push(c); }, openTeamConversation: async () => {} }, async () => {
    assert.equal([...document.querySelectorAll('button')].find(b => b.textContent === 'Vote for Builders')!.disabled, true);
    assert.match(document.body.textContent!, /Awaiting administrator/);
    await click('Close submissions and open voting');
    assert.deepEqual(calls, [{ action: 'submissions_close' }]);
    assert.equal([...document.querySelectorAll('button')].find(b => b.textContent === 'Vote for Builders')!.disabled, true);
  });
});

test('native copy keeps interests editable and opens actual submitted files', async () => {
  const calls: unknown[] = []; const opened: string[] = [];
  await mounted(Hackathon, { hackathon: { ...state(), profiles: { me: 'Design' } }, identityId: 'me', isAdmin: true, isJudge: false, command: async (c: unknown) => { calls.push(c); }, openTeamConversation: async () => {}, openFile: async (id: string) => { opened.push(id); } }, async () => {
    assert.equal(document.querySelector<HTMLInputElement>('input[maxlength="512"]')!.value, 'Design');
    await input('What would you like to work on?', 'Interfaces'); await click('Update interests');
    assert.deepEqual(calls, [{ action: 'register', skills: 'Interfaces' }]);
    await click('Open submitted file'); await click('Open project file'); assert.deepEqual(opened, ['existing', 'existing']);
    assert.equal([...document.querySelectorAll('button')].find(b => b.textContent === 'Publish winners announcement')!.disabled, true);
    assert.deepEqual([...document.querySelectorAll('h2')].map(n => n.textContent), ['Overview', 'Find teammates', 'Schedule', 'Project', 'Judging & voting', 'Winners']);
  });
});

test('membership page creation uses authorized partition and clears only on confirmed success', async () => {
  const calls: string[][] = []; let reject = true;
  const props = { tiers: [{id:'tier',name:'Builder',space:'partition',members:[],groups:[],resources:[]}],identityId:'me',isAdmin:true,currentSpace:'home',command:async()=>{},createRestrictedPage:async(space:string,title:string)=>{calls.push([space,title]);if(reject)throw Error('Rejected');} };
  await mounted(CommunityTiers,props,async()=>{
    await input('Page title','Garden guide'); await click('Create members page');
    assert.equal(document.querySelector<HTMLInputElement>('input[name=pageTitle]')!.value,'Garden guide');
    reject=false; await click('Create members page');
    assert.equal(document.querySelector<HTMLInputElement>('input[name=pageTitle]')!.value,'');
    assert.deepEqual(calls,[['partition','Garden guide'],['partition','Garden guide']]);
  });
  for(const isAdmin of [true,false]) await mounted(CommunityTiers,{...props,isAdmin,tiers:[{...props.tiers[0],space:null}]},async()=>{
    const button=[...document.querySelectorAll('button')].find(b=>b.textContent==='Create members page');
    if(isAdmin){assert.equal(button!.disabled,true);await act(async()=>{button!.closest('form')!.dispatchEvent(new dom.window.Event('submit',{bubbles:true,cancelable:true}));});}else assert.equal(button,undefined);
  });
  assert.equal(calls.length,2);
});
test('invitation expiry uses entered local time and rejects past dates',async()=>{
  const calls:unknown[]=[];
  await mounted(CommunityTiers,{tiers:[{id:'tier',name:'Builder',space:'partition',members:[],groups:[],resources:[]}],identityId:'me',isAdmin:true,currentSpace:'home',command:async(c:unknown)=>{calls.push(c);}},async()=>{
    await input('Invite expiry','2000-01-01T12:30');await click('Create invitation');assert.equal(calls.length,0);
    await input('Invite expiry','2099-06-12T14:30');await click('Create invitation');
    assert.deepEqual(calls,[{action:'tier_invite',tier_id:'tier',expires_at_ms:new Date('2099-06-12T14:30').getTime()}]);
  });
});

test('confirmed feedback is silent initially and pending, then draws only after success', async () => {
  let resolve!: () => void;
  await mounted(Hackathon, { hackathon: state(), identityId: 'me', isAdmin: true, isJudge: false, command: () => new Promise<void>(r => { resolve = r; }), openTeamConversation: async () => {} }, async () => {
    assert.equal(document.querySelector('.zh-feedback'), null);
    await click('Update interests');
    assert.equal(document.querySelector('.zh-feedback'), null);
    await act(async () => resolve());
    assert.equal(document.querySelector('[role=status]')?.textContent, 'Changes saved.');
    assert.ok(document.querySelector('.zh-feedback svg circle'));
    assert.ok(document.querySelector('.zh-feedback svg path'));
  });
});
test('failed mutation has negative feedback and claims never fall back to raw identifiers', async () => {
  await mounted(CommunityTiers, { tiers: [], identityId: 'me', isAdmin: true, currentSpace: 'home', claims: [{ identity_id: 'person-secret', invite_id: 'invite-secret', tier_id: 'tier-secret' }], command: async () => { throw Error('Action failed.'); } }, async () => {
    assert.ok(document.body.textContent?.includes('requests Membership'));
    assert.ok(!document.body.textContent?.includes('tier-secret'));
    await click('Approve membership request');
    assert.equal(document.querySelector('[role=alert]')?.textContent, 'Action failed.');
    assert.equal(document.querySelector('[role=status]'), null);
    assert.ok(document.querySelector('[role=alert] svg path'));
  });
});
