# ADR 0026: Live pages: documents that people and agents keep up to date together

Status: proposed (2026-10-08). Depends on ADR 0025 (editor, Loro), 0027 (dynamic UI), 0014
(memory, files and knowledge as Items), 0013 (hooks), the agent runtime ADR (zoen-agentd on
Rig) and the approvals API (`docs/api-approvals.md`, branch `ui/approvals-swipe`).

## Context
Enzo pointed at ChatGPT Space (chatgpt.com/features/space, launched 2026-09-29). From the
feature page, help center and coverage, here is what it offers:
- Pages are block documents you edit with teammates and ChatGPT in real time.
- The `/` menu has tables, code, files, links to pages and chats, "Visualize" (interactive
  calculators and diagrams), and Prompt, Task and Agent-instructions blocks.
- You can mention ChatGPT in a comment to edit that part of the page.
- Pages "can stay updated from your connected tools".

What it doesn't do yet, per OpenAI's own help center as summarized by eesel.ai on 2026-10-01:
- "Keep updated" was **not available at launch**; recurring updates are scheduled tasks.
- Mobile can only read.
- The server reads everything.

Similar products:
- **Notion AI pages:** AI blocks, databases with synced views.
- **Coda:** formulas and packs bind tables to live data.
- **Microsoft Loop:** components synced across apps.

Enzo ties this to TextQL's ideas, studied earlier (`/workspace/refs/textql/FINDINGS.md`, ideas
only, no text or code; neither repo carries a usable license):
- an ontology of plain files (entities, governed metrics, queries, skills) that the agent reads
  through small routing tables, extends, and proposes changes to for review;
- golden values that act as regression tests for meaning.

The Muse runtime-export incident is the counter-lesson. An agent zipped its own runtime,
keys included, and sent it out. Export must carry the user's data and definitions, never a
runtime.

## Decision
A **live page** is a Page (ADR 0025) that lives in a Space, where some blocks are bound to
data and kept fresh by members or agents.

### Blocks
The page body is the native WYSIWYG editor's block tree. A **live block** is a block with:
- `source`: what it reads. One of:
  - an ontology **metric** or **entity** Item;
  - a saved **query** Item;
  - the Space's **tasks** or **plan**;
  - a **message** range or thread;
  - a **file**;
  - an **agent section**, meaning prose an agent maintains from a skill and instructions.
- `view`: how it looks, a declarative Zoen View (ADR 0027) such as a metric tile, table,
  chart, task list, message excerpt, or the agent's formatted text.
- `refresh`: one of
  - "on change": re-derive when the source Item gets a new version;
  - a schedule ("toda segunda 9h");
  - "manual".
- `keeper`: the member (a person's device or an agent) responsible for refreshing it.
- `value`: the last materialized result, plus its **provenance**: the `(space, seq, hash)` of
  every event it was computed from, and the skill or definition version used.

Live blocks are Loro sub-documents, so people can keep typing around them while an agent
updates one.

### Ontology behind the page
The Space's ontology is made of ordinary Items (ADR 0014), under paths such as:
- `ontology/entities/*`
- `ontology/metrics/*`: a typed definition with formula, inputs, unit and owner;
- `ontology/queries/*`
- `skills/*`: Agent Skills format;
- `ontology/ROUTING.md`, the small routing index agents always load.

A metric block renders the governed value, and tapping it opens the definition and its
history. Golden values attached to a metric are checked on every refresh. A mismatch doesn't
publish; it raises an approval card ("Receita mudou de definição?").

When an agent learns something new, such as a better definition, it proposes an Item change
through the request flow. It never edits definitions silently.

### Who edits, and E2E
- People edit on their devices.
- Agents edit only as **declared members** of the Space. They hold an MLS leaf and run either
  on a member's device or in that member's agent runtime (zoen-agentd). That's where they
  decrypt, compute and write a signed Loro update.
- The relay never computes a block, never sees a value and never schedules with content. A
  schedule is a sealed Item, and the keeper's runtime wakes on its own timer. The relay only
  sees ciphertext and metadata, under the ADR 0021 rules.
- A refresh writes an event only when the value's hash changes. A refresh that changes nothing
  writes nothing.

### Approvals and grants
- Every agent edit is attributed: the margin shows the agent's hand-drawn avatar, and
  Versões shows "Financeiro atualizou Receita do mês".
- Whether an edit applies directly or waits is a Cedar decision on
  `(agent, action: page.edit, resource: page/section, Space trust level)`:
  - **Ouvir/Sugerir:** every edit is a suggestion shown inline and as a swipe card.
  - **Agir:** edits inside the agent's own sections apply directly with Desfazer; edits
    elsewhere are suggestions.
- On the swipe cards (approvals API):
  - right approves this edit;
  - up approves always, which writes a Cedar grant scoped to that agent, page and section;
  - left denies;
  - down denies always.
- An approval is bound to the content hash of the proposed update, so an approved edit can't
  be swapped afterwards.
- An agent edit can never change who can read the page, add members or issue grants (ADR 0014
  rule).

### Hooks
These event kinds are added to the ADR 0013 catalog:
- `page.updated` (before and after; payload: page, blocks touched, author);
- `page.block.refreshed` (after);
- `page.suggestion.opened` / `page.suggestion.resolved`.

Device and agent-member hooks see content. Server hooks get the content-free `MetaEvent` only.
Examples:
- a `before` hook that blocks an agent edit containing a CPF;
- an `after` hook that posts "a página Orçamento mudou" in the chat.

### Export
"Exportar página" builds a signed bundle on the device containing:
- `page.md`;
- each live block's binding, view and last value with its proofs;
- the ontology definitions and skills it depends on;
- a `manifest.json` listing them.

Another Zoen, or any tool that reads the files, can re-run the definitions. That's the
portable runtime: **declarations, not a machine image**. The bundle never contains:
- host files, keys, tokens or connector credentials;
- the agent's sandbox, runtime or caches.

Only the owner starts an export, and sending it anywhere is an external action that always
asks for confirmation (ADR 0014).

### Feel
- An updating block shimmers once (the ink-reveal style).
- A changed number rolls to its new value with a soft haptic.
- A suggestion appears as hand-drawn underlines in the agent's color.
- Accepting a suggestion snaps it in with a success haptic.
- Nothing is labelled "CRDT", "sync" or "query". Users see "Atualizado há 2 min por
  Financeiro" and "Manter atualizado".

## Why not just copy ChatGPT Space
- ChatGPT Space keeps pages fresh through its server, with plaintext access. Zoen does the
  same on member devices and agent members, under E2E encryption.
- ChatGPT pages are read-only on mobile. Zoen's editor is native on both platforms from the
  first slice.
- Zoen agents are accountable: every change has a signed author, provenance, an approval card
  and Desfazer.

## First slice
1. `ItemKind::Page` on Loro with the native editor (ADR 0025 phase 1).
2. Two live block sources:
   - **Tarefas deste Espaço**, computed on the device, no agent;
   - **Métrica**, bound to an `ontology/metrics/*` Item with a golden value, kept by an agent
     member on "on change".
3. An **agent section** ("Resumo da semana") refreshed manually by "Atualizar", proposed as a
   suggestion, and approved with the swipe card. "Aprovar sempre" writes the scoped Cedar
   grant, and the next refresh applies directly with Desfazer.
4. Versões with author attribution, and the `page.updated` after-hook on the device.
5. Journeys:
   - CLI: two people and an agent member through the relay. The agent's suggestion waits,
     approval applies it, the second device sees it, and the relay's log holds no plaintext.
     A golden-value mismatch raises a card instead of publishing.
   - App (one simulator): a person approves the agent's edit by swiping right and sees
     "Atualizado por Financeiro".

## At 1B users
- Assume 100M live blocks with "on change" refresh, averaging 5 changes a day: 500M small
  updates a day (about 6k/s). That's a fraction of message traffic (ADR 0022: about 2,500
  appends per FoundationDB core) and adds about 2 cores per cell.
- Scheduled agent sections are what cost money: model tokens, charged to the keeper's owner
  budget (`UsageRecorded`).
  - Routing-first loading (ADR 0014) keeps a refresh around 4k input tokens.
  - Hash-gated writes mean an unchanged section costs no storage.
  - A daily agent section on 10% of people is 100M runs a day, which belongs in the agent
    budget, not the relay budget.
