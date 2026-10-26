# Generated art: avatars for agents and photos for groups

Status: design. Ready to implement once Enzo picks from the `avatars-v1` sheets.
Owner surfaces: the iOS/macOS app (picker and on-device distiller), `zoen-art` (a new service) and `zoen-media` (existing).

Style reference implementation:
- module: `zoen-assets/pipeline/zoen_style.py`, preset `v1-vivid` (default), `STYLE_VERSION = zoen-ink-wash/1`
- CLI: `zoen-assets/pipeline/zoen_art.py`
- docs: `zoen-assets/pipeline/README.md`

This is the visual/product policy. Android parity work is still changing in the owner's checkout and is not certified by earlier Android CI. Use the [version ledger](roadmap-status.md#version-ledger) and actual attached visual journeys.

## 1. What the user sees

| Who | Default picture | "Gerar" (Generate) |
|---|---|---|
| A person | their own photo, uploaded by them | **never**. We do not generate faces for people. No photo: a monogram (initials, tint derived from the account id, see section 9) |
| An agent (bot) | one of the 24 hand-drawn agents in `avatars-v1`, picked by tag match on the agent's description | the agent's owner taps **Gerar** and gets 3–4 options in Zoen style |
| A group or Space | one of the 22 hand-drawn group photos, picked by tag match on the name | any member allowed to edit the group photo taps **Gerar** |

Flow (about 10 s end to end, nothing blocks chat):

1. Tap **Gerar** on the agent or group photo editor.
2. A sheet shows **3–5 theme chips** that Zoen proposes from context: "praia", "violão", "amigos", "fim de semana". Chips are editable: remove, add your own (free text, max 5 chips, 24 chars each). A small line says: *"Só estas palavras saem do seu aparelho."* ("Only these words leave your device.")
3. Tap **Criar** (Create). Four paper cards appear and draw themselves in ink (skeleton shimmer is a pencil sketch that boils), with a soft haptic as each lands.
4. Pick one; it goes through the normal encrypted media upload. **Mais opções** (more options) asks again with new seeds (counted against the quota). Tapping a chip set already generated before returns instantly from cache.
5. The picture animates (line boil) in the profile and the chat header; lists show the still. Reduce Motion always shows the still.

## 2. Privacy model (E2EE stays intact)

Zoen's group content is end-to-end encrypted (MLS). The server never sees messages, and it must not start
seeing them through the side door of "smart" art. So:

- **Context is read only on the device**: group name, group description, the last N (default 200) message
  texts the user can already decrypt, pinned items, the agent's description and its declared skills.
- **An on-device distiller** turns that into theme keywords (section 3). Raw text never leaves the
  device, never enters a prompt and never goes to logs.
- **The user sees and can edit the keywords before anything is sent.** Nothing is sent without the tap on Criar.
- **What leaves the device**: `{kind: agent|group, keywords: [≤5 short words], locale, seedHint, styleVersion}`.
  The server assembles the prompt from a fixed template (section 4). The client never sends a free-form
  prompt, so it can't smuggle message text into one.
- **Keyword guard on device**, before the chips are shown, drops anything that looks personal:
  - names from the group's member list and the contact book
  - phone numbers, emails, URLs, @handles, numbers longer than 3 digits
  - words on the sensitive list: health conditions, religion, sexuality, politics, minors, addresses
  The user can still type such a word by hand. The server classifier (section 6) then decides.
- **Transport**: the request goes over the normal authenticated device channel. The art service stores
  **no** account id next to keywords. The rate limiter keys on an HMAC of the account id, held in a
  separate store with a 24 h TTL.
- **The result is encrypted like any media**: the client downloads the 4 previews, picks one, then
  encrypts it with a fresh per-file key and uploads through `zoen-media`, exactly like a photo. The group sees only
  the encrypted blob reference in the MLS group context. The art service forgets the picks. Unpicked
  candidates expire from its cache after 24 h and are never linked to the group.
- **Logs and analytics**: count, latency, provider, cost, moderation outcome. Never keywords, never images.
  Privacy-preserving analytics (point 10) aggregates keyword *categories* only (e.g. "sport"), with k ≥ 50.

## 3. On-device distiller

Goal: 3–5 concrete, drawable nouns or scenes, not abstract feelings or people.

**Pipeline, all on device:**
1. **Gather** text: name, description, recent messages (text only, no media), pinned item titles.
   Agents use the description and skills instead.
2. **Candidate terms**: `NLTagger` (lemma + lexical class) keeps nouns and noun phrases. Drop stopwords and
   member/contact names (`NLTagger` `.nameType` plus the member list). TF-IDF against a bundled
   background frequency table, 40k lemmas per supported language (pt-BR, en, es), weighted so the
   name and description count 5x a message.
3. **Map to drawable themes**: embed candidates with `NLEmbedding.sentenceEmbedding` (or the bundled
   small embedding table on platforms without it). Take the nearest entries in the **theme vocabulary**, about
   600 curated drawable themes such as `beach`, `guitar`, `soccer`, `campfire`, `ramen`, `laptop`, `cat`,
   `mountains`. Each entry has localized labels, a safety class, and the default-library assets that
   match it. Cosine ≥ 0.45 required.
4. **Diversify**: pick 3–5 with maximal marginal relevance, so not five beach synonyms.
5. **Agents** also get a role theme from the skill manifest (travel → `map`, coding → `laptop`).
6. **Apple Intelligence (iOS 26+), when available**: the on-device Foundation Models framework reranks
   or rephrases the chips with guided generation constrained to the vocabulary enum. It is an optional
   enhancer, so the deterministic path above always works and keeps results identical across devices.

Because chips map to a **closed vocabulary** (plus user-typed extras), the same intent produces the same
canonical keywords. That is what makes the server cache effective (section 7).

**Default picks with no generation**: the same chips select the best default-library asset by tag
overlap. A new group gets a fitting hand-drawn photo instantly, offline, at zero cost.

## 4. Server: `zoen-art`

[Service architecture and pipeline](design-art-service.md#4-server-zoen-art).

## 5. Rate limits and cost

| Limit | Value (start) | Why |
|---|---|---|
| Generations per account per day | 5 requests (= 20 candidates) | free-tier capacity, abuse |
| Per group or agent per day | 3 requests | stops a whole group hammering one target |
| Concurrent per account | 1 | queue fairness |
| Global provider budget | staging: Workers AI ledger capped at 8,500 neurons/day (section 4); prod: GPU queue capacity | can never bill; never overload |
| Staging per account | 2 requests/day, 3 candidates each | fits the free allocation |
| Cache hit | free, unlimited | most requests |

Requests over the limit get a friendly message with a countdown and still see the default-library suggestions.

**Cost per generation** (4 candidates):
- Staging (Workers AI free allocation): 0 USD by construction, because of the hard cap. Our style CPU
  adds about 4 × 2 s for previews plus 15 s for the pick, roughly 0.00003 USD.
- If Workers AI were billed, it would be 4 × 172.8 neurons ≈ 0.0076 USD per request. That's only a
  reference point; we don't plan to buy it.
- Production, self-hosted FLUX.1-schnell on one L4-class GPU (~0.8 USD/h, ~1 s per image at 512, 4 steps):
  ~0.0009 USD per request, plus style CPU.

**Cost per user** (feeds `docs/cost-model.md`):
- Assume 2% of MAU generate once a month, 1.5 requests each.
- At a 60% cache hit rate (closed vocabulary), that's 0.012 paid requests per MAU per month.
- Self-hosted at 1B MAU: 12M requests a month × 0.0009 USD ≈ 11k USD a month, i.e. 0.000011 USD per MAU.
- On a metered API it would be 90k–600k USD a month: about 0.0076 USD per request on Workers AI, up to
  0.05 USD on others. That's why production self-hosts. Storage is small: ~200 KB per picked asset, and
  unpicked candidates expire.

## 6. Moderation

Generated art is shown to whole groups, so it is moderated twice, on input and on output.

- **Input**:
  - Keywords must be either vocabulary entries, which are pre-classified safe, or free words that pass a
    multilingual text classifier: a small open model in-cluster, plus a blocklist per locale.
  - Blocked: sexual content, minors, violence or gore, hate symbols, real people and celebrity names,
    brands and trademarks, political figures.
  - The template also forces "no text, no letters", which keeps out slurs rendered as text and copied logos.
- **Output**:
  - Every base image goes through an NSFW and violence image classifier (open-weights, in-cluster).
  - A face detector runs too. A **realistic human face is rejected** for agents and groups, consistent with "people are photos, never generated".
  - Illustrated creatures pass.
  - Rejects are retried once with a new seed, then the slot is dropped (show 3 instead of 4).
- **Provider-side** filters, where a provider has them, are a third net. We never rely on them: Workers AI
  FLUX-schnell has no output filter we can depend on, and self-hosted has none. Our own input and output
  classifiers are the contract.
- **Reports**: a group photo can be reported like any media. The report carries the blob commitment
  (security.md, moderation commitments), so moderation can verify what was shown without the server ever holding plaintext by default.
- **Audit**: a moderation decision log keyed by request hash. No keywords are stored after 24 h except
  hashed blocklist hits (for abuse-pattern detection).

## 7. Scale to 1B users

- **Cache first**: the key is `H(STYLE_VERSION, kind, sorted canonical keywords, n)`. It's stored in
  NATS KV (index) and the object store (bytes), with a CDN for previews.
  - Closed-vocabulary keywords make popular themes (beach + friends, soccer, family) converge. We expect
    over 60% hits after warm-up.
  - We **pre-warm** the top 2k keyword combos offline at night when providers are idle.
- **Queue**: JetStream `ART` work queue with `art.p0`, `art.p1` and `art.batch` subjects, at least once and
  idempotent by request hash.
  - Provider workers are I/O-bound and run thousands of concurrent async jobs.
  - Style workers are CPU pods on an HPA driven by queue depth.
  - The client gets candidates by long-poll or push on its existing channel. No synchronous request is
    held open more than 25 s; past that, "we'll notify you".
- **Back-pressure**: when the queue age is over 60 s, new requests get the default-library picks plus "try later".
  Chat is never affected (system-design.md goal 2: chat works when media/agents are down).
- **Multi-region**: stateless; cache objects are immutable and replicated by the CDN; each region has its own queue.
- **Abuse at scale**: per-account limits keyed by HMAC, device attestation (App Attest) on the request,
  and global anomaly alarms on request rate by keyword category.

## 8. Rollout

1. **M-art-0** (now): default library `avatars-v1`. Tag-based default picks on device, no server work.
   Monogram fallback for people.
2. **M-art-1**: the on-device distiller and chips, with the chips choosing only from the default library
   (zero cost, fully offline). This validates the UX.
3. **M-art-2 (staging)**: `zoen-art` with the `WorkersAiProvider` on the free allocation, under the
   hard cap. It ships with quotas, cache, moderation and the encrypted upload path, for agent owners
   first, then group admins.
4. **M-art-3 (production)**: the self-hosted FLUX.1-schnell GPU pool, switched on by config
   (`ART_PROVIDER=self-hosted`) once a budget is approved, plus the Rust port of the style workers.
   Workers AI stays configured as a capped overflow only if Enzo approves a paid budget for it; otherwise
   it's disabled in production.

## 9. Monogram fallback for people (UI worker)

- 1–2 initials (first + last grapheme cluster, uppercased, locale-aware), in the app's serif at 0.42 of
  the diameter, ink colour `#2B2622` on light and paper `#F3EDE1` on dark.
- Background is the paper disc tinted with one of 9 palette hues. The hue index is
  `H(accountId) mod 9`, stable across devices and never derived from the name, so renames keep colour.
  Palette:

  `#7FAF6B #A8C3A0 #D9825B #E3B55B #8DB7D4 #E5A4A0 #9B7FB0 #D8C3A0 #6F9FA8`

- Static, with no boil. Movement is reserved for drawn art.

## 10. Open questions for Enzo

- Daily quota (5) and who may generate a group photo (admins only vs any member).
- Whether to show which provider made the base image (transparency) or keep it invisible.
- Apple Intelligence chip rewriting on by default, or only the deterministic path?
- Create a Cloudflare API token scoped to *Account › Workers AI › Read* for staging, the same way as the
  DNS token. Also: is the account on Workers Free or Workers Paid? On Free, Cloudflare itself also stops at 10,000 neurons.
- ~~v1 vs v1-vivid~~: decided on 2026-10-08. `v1-vivid` is the default and the set was re-rendered with it.

## 11. Provider, licences and terms

**Decision (Enzo, 2026-10-08):**
- AI Horde is **not** used for user requests, because of the prompt sharing with LAION and the profit-share clause.
- Staging uses Cloudflare Workers AI `@cf/black-forest-labs/flux-1-schnell` on the free allocation, under
  a hard cap.
- Production self-hosts FLUX.1-schnell.
- The adapter keeps the switch a config change.

**What avatars-v1 actually used:**

- **Base art:** AI Horde (aihorde.net), anonymous, model `Flux.1-Schnell fp8 (Compact)`, 512 px, 4 steps.
  46 of the 48 subjects were delivered. Two were censored three times and dropped. Every asset was then re-rendered by `zoen_style` preset `v1`.
- **FLUX.1 [schnell] licence:** Apache-2.0 (Black Forest Labs). Personal, scientific and commercial use
  are allowed. Outputs are not "Derivative Works" of the model, so no notice is needed on the images.
  We'd keep the licence notice only if we redistribute the weights (self-hosting in M-art-3).
- **AI Horde terms:**
  - The service "claims no rights on the outputs you generate; you are free to use them and are accountable for
    their use".
  - Its page names the CreativeML Open RAIL-M use restrictions (no illegal content, harm, harmful
    personal-data use, misinformation, or targeting vulnerable groups). These are written for SD models;
    FLUX-schnell's own licence is Apache-2.0. Our use (mascots, objects) fits both.
  - **Anonymous text2img prompts and outputs are shared with LAION.** The prompts here were generic
    ("a cute owl wearing a graduation cap…"), so that's harmless for the default set. It is not acceptable for
    user requests (see section 4).
  - Volunteer workers can see prompts.
  - Kudos can't be bought or sold.
  - Horde asks that **paid or ad-funded integrations give back**. If an app relies solely on Horde, it
    expects at least 50% of the related profit to go to supporting it, preferably by running our own workers.
  - Together with the LAION sharing, this is why Horde is ruled out for anything user-facing.
  - Using it once to make the default library is within its terms. The prompts were generic, and Zoen
    isn't a paid integration of Horde. If Zoen ever wants to give back anyway, running a worker on idle
    GPU time in M-art-3 is the natural way.
- **Cloudflare Workers AI (staging):**
  - The model's own licence applies; FLUX.1-schnell is Apache-2.0.
  - Inputs and outputs are our Customer Content. We own them, and Cloudflare doesn't train on them or share them.
  - Usage runs under Enzo's existing Cloudflare self-serve agreement, within the free allocation.
- **Pollinations:** tried and not used for any delivered asset.
- **Copyright caveat:** purely AI-generated images generally can't be copyrighted. The US Copyright Office
  requires human authorship, and Brazil's LDA 9.610/98 protects works by natural persons. Our deterministic
  re-render probably doesn't change that. So Zoen can use and ship these assets freely, but can't stop others
  from copying them. If exclusivity matters for the mascot (the green furball), have a human illustrator
  redraw the hero version.
- **Likeness and trademarks:** the set is creatures and objects only, with no people, no logos and no text.
  Generated faces of people are out of scope by design (section 1).
- **No keys were used for avatars-v1.** None of Enzo's, and no paid tier: the Horde key was the public anonymous key.
  Nothing was added to `docs/accounts.md` because no account was created.
- **Mascot copyright:** if exclusivity matters for the green furball, **a human illustrator should redraw
  it**. Use the generated furballs (`agent-furball-*`) as the brief. The human version becomes the hero
  asset and the trademark specimen.
