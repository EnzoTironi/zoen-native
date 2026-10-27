# Generated art service

[Generated art design](design-art-generation.md#4-server-zoen-art)

## 4. Server: `zoen-art`

A new stateless service in the same family as `zoen-media`:

```
client ──(device channel)──► edge ──► zoen-art API ──► cache lookup (KV)
                                          │ miss
                                          ▼
                                 JetStream work queue ART (per-priority subjects)
                                          │
                       ┌──────────────────┴───────────────────┐
                 provider workers (I/O-bound)          style workers (CPU, zoen_style)
                 adapter → base image                  base → Zoen look → frames → exports
                                          │
                              R2/Tigris (content-addressed, private bucket) ─► CDN signed URL (previews, 24 h)
```

- **Prompt assembly** (server-side, fixed template):
  `"{subject from keywords}, " + STYLE_PROMPT[kind]`, where the subject comes from vocabulary entries'
  curated English phrases (e.g. `guitar` → "an acoustic guitar with a few floating music notes"). Free-typed
  keywords are slotted in as plain nouns after moderation. `STYLE_PROMPT` and negative terms are versioned
  with `STYLE_VERSION`.
- **Provider adapter**: `trait ImageProvider { fn generate(&self, prompt, seed) -> Result<Bytes>; fn cost(&self) -> Cost }`,
  with health, cost per image and a circuit breaker per provider. **Which provider is used is configuration**
  (`ART_PROVIDER=workers-ai|self-hosted`, plus per-provider budgets), so switching never needs a code change.
  Both planned providers run the same model, FLUX.1-schnell, so the style tuning carries over unchanged.

  | Environment | Provider | Model | Spend |
  |---|---|---|---|
  | **staging** | `WorkersAiProvider`: Cloudflare Workers AI on Enzo's existing Cloudflare account, called only from `zoen-art` | `@cf/black-forest-labs/flux-1-schnell`, 4 steps | **free daily allocation only**, under a hard cap (below) |
  | **production** | `SelfHostedProvider`: our own GPU pool (`zoen-art-gpu`, diffusers or ComfyUI headless on L4-class nodes) | FLUX.1-schnell weights (Apache-2.0), 512 px, 4 steps | per the cost model in section 5 |
  | dev and offline tests | `FixtureProvider`: canned base images from `avatars-v1/_raw` | none | 0 |

  - **What leaves our infrastructure.** Only the server-assembled style prompt, built from the distilled,
    user-approved, non-sensitive keywords, plus a seed. No messages, account ids or group ids go out.
  - **Cloudflare's data terms.** Workers AI treats inputs and outputs as the customer's content. Cloudflare
    says it doesn't share that content with other customers and doesn't use it to train models or improve
    services without explicit consent. It stores nothing unless we pair Workers AI with one of its storage
    products. We don't: the result goes straight into our own pipeline and bucket.
  - **Workers AI cost.**
    - The model has no size parameter and always returns 1024×1024, which counts as 4 tiles of 512×512.
    - One image at 4 steps costs 4 × 4.8 + 4 × 4 × 9.6 = **172.8 neurons**. That's about 0.0019 USD at the list
      price of 0.011 USD per 1,000 neurons, but **0 within the free allocation of 10,000 neurons a day**.
    - That allocation is about 57 images, or 14 requests of 4 candidates.
  - **Hard daily cap so it can never bill.**
    1. `zoen-art` keeps a ledger in NATS KV (`art.budget.workers-ai.<UTC date>`) and reserves 172.8 neurons
       *before* each call, using a compare-and-set.
    2. When a reservation would push the day past **8,500 neurons**, the provider is refused. That leaves a
       15% margin for anything else on the account that uses Workers AI.
    3. An hourly reconciler reads the account's real Workers AI usage from Cloudflare's GraphQL analytics.
       If the account-wide total for the day is over 8,500, it trips the same breaker, even if someone else
       used the neurons.
    4. While the breaker is open, the request falls back to the default-library picks plus "volte amanhã"
       ("come back tomorrow"). Requests never queue for a paid overflow.
    5. Staging serves **3 candidates per request** (518 neurons), which gives about 16 requests a day. The
       staging quota is 2 requests per account per day.
    6. If the account is on Workers Free, Cloudflare itself refuses calls beyond 10,000 neurons, which is a
       second wall. If it's on Workers Paid, our cap is the wall, so the cap is a CI-tested invariant
       (a unit test proves a reservation over the cap is refused).
  - **Token.** It needs a separate API token scoped only to *Account › Workers AI › Read*. The existing
    DNS and R2 token isn't reused. It's stored through External Secrets, and Enzo creates it the same way
    as the DNS token. The cap logic doesn't depend on the token's scope.
  - **Not used for user requests: AI Horde and Pollinations.**
    - AI Horde: anonymous prompts are shared with LAION, volunteer workers see prompts, and paid integrations
      are asked to give back at least 50% of related profit (section 11). It stays a one-off tool for
      offline asset work only.
    - Pollinations: throttled, watermarked, and it follows the style weakly.
  - **No Enzo model keys and no paid tiers** until a budget is approved. Production self-hosting is the paid
    path, and it starts only after a budget decision.
- **Style pipeline** (`zoen_style`): the provider is only a source of shapes and colours. Every result is
  re-rendered: subject mask, recentred circle-safe or organic-vignette crop, Zoen palette unification, ink
  re-drawn from a skeleton with variable width, translucent washes with edge darkening, shared paper
  texture, then 3 boil frames. This is why any model, now or later, produces the same family look. Port plan:
  keep Python (numpy/opencv/scikit-image) in a container first. Measured: 12–15 s of CPU per asset on one
  core (3 frames at 1024). Previews can render at 512 with 1 frame (about 2 s) and the full asset only for
  the pick. Port the hot loops to Rust (`image`, `imageproc`) if CPU cost matters.
- **Outputs per candidate**: still PNG 512, animated WebP 384 (~30–90 KB), MP4 H.264 384 fallback, and
  `{dominant, accent, tint}` colours, the same contract as `manifest.json` in the default set.
- **Seeds**: deterministic `seed = H(canonical keywords, kind, STYLE_VERSION, n)`, so identical requests
  hit cache and "Mais opções" bumps `n`.
