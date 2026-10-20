# Telegram feature matrix

Requested scope: a Telegram-only inspiration catalog, including Premium and the wider ecosystem, with frontend references for later study. This package does not choose a Telegram backend, implement a connector, fork a client, change Zoen's UI, or define which features Zoen should adopt.

**Primary UI reference: Telegram iOS**, as explicitly requested by the user. Start implementation study with its flows and code entrypoints; Android is secondary. The durable [product direction](../../docs/product/telegram-ios-reference.md) preserves this decision for later implementation work.

Reference date: **20 October 2026**. Public API configuration: **layer 225**. Official client source pins: iOS `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838` and Android `0aae959806c57be8ad457980f2a5c6277ff18635`. Published source and installed application versions can differ; the earlier [freshness review](../telegram-research-2026-10-20/latest-code-review.md) records that distinction.

## Open the matrix

- [Searchable feature matrix](index.html): search, filter by area or availability, inspect official references and expand frontend entrypoints.
- [Readable matrix](matrix.md): the same capability rows organized into 23 areas.
- [Feature CSV](matrix.csv): editable catalog with behaviors, gates, evidence and pinned frontend links.
- [Full JSON](matrix.json): features, Premium/Business registers, source enums, quota parsers and historical announcements.
- [Premium and Business CSV](premium-business-register.csv).
- [Limits, source catalogs and upgrade entrypoints CSV](limits-and-entrypoints.csv).
- [Historical announcement CSV](announcement-register.csv).

## Coverage

The main catalog contains **506 capability rows in 23 areas**: messaging/composition; formatting; reactions/stickers/emoji; media/attachments; voice; calls/streams; inbox/folders/Saved Messages; search/reading; groups/topics/communities; channels; polls/checklists; stories; profiles/contacts; privacy/security/account lifecycle; Business; bots/agents; Apps; AI/translation; Premium; boosts; commerce/gifts; appearance/accessibility; storage/connectivity/platform integration.

It preserves the complete finite registers returned by the captured official machine-readable API configuration: **29 Premium identifiers, 21 Premium quota categories, 7 Premium story extensions and 11 Business identifiers**. These describe overlapping product facets, not 68 additional unique features. The public configuration endpoint documents enum definitions; it does not return a user's live quota values.

The pinned iOS source separately contributes **35 Premium perk cases, 50 subscription entrypoints and 30 configurable limit-parser fields**. These are fully enumerated under “Limits & entrypoints” and retained in JSON. They are not directly comparable denominators to the API identifiers: Business categories, combined UI categories, quotas and upgrade surfaces overlap.

All **676 unique announcement candidates** from the earlier official release/evolution sweep are retained in the Release archive. Matched URLs identify related catalog rows, not exact semantic equivalence. Unmatched material remains visible for further study; it is not silently classified as removed. Old quotas, programs, metrics and platform releases remain explicitly distinguishable from current capabilities.

This is broad public-product coverage, with explicit completeness for those captured registers. It is not a claim to enumerate hidden server functionality, every RPC/SDK method, every rollout or every OS-specific gesture. A feature announcement is evidence of documented behavior, not a new runtime pass. All product behavior remains untested in this research-only delivery.

## How to interpret gates

“Standard” means no subscription gate was stated in the cited feature description. It does not guarantee current universal availability. Roles, counterpart privacy, content protection, region, app version, configured thresholds and service availability can still restrict a feature.

Premium, Business, group/channel boost levels, paid transactions and client/OS support are separate conditions. Creating something and viewing or participating in it may have different gates. Connected profile/business bots are documented as available beyond Premium; that does not make every Business feature free.

Numeric quotas are preserved as identifiers or source fallbacks. Live free/Premium values were not queried. The Premium “doubled limits” label is not applied as a universal formula. Publicly advertised multipliers, such as the current AI text allowance, are identified as published claims rather than account measurements. Subscription prices were not queried because prices vary by billing route and region; pricing is not needed to enumerate the feature set.

## Frontend study

Each area has iOS and Android entrypoints whose filenames were checked against the complete pinned repository trees. These are **area-level source leads**, not verified per-feature traces, extracted portable components or proof of standalone reuse. Backend-dependent services, entitlements, storage and account operations do not become implemented by copying UI.

The future implementation choice remains open: reproduce a behavior using Zoen components, adapt upstream UI with its dependencies, or change the UI after reviewing the relevant feature. Direct code reuse requires checking that component's license and dependencies at the selected revision. This package does not change the product's license or adopt upstream assets.

## Sources and reproduction

`source-receipts.json` records the 164 captured official pages, including preserved same-day snapshots and fresh public-document requests. `sources/config-receipt.json` pins the official `api/config.json` capture. `link-adjustments.json` records malformed or missing fragment anchors that were corrected or replaced with a link to the captured source page. No account, contact, chat history, credential or private content was accessed.

`catalog.txt` is the curated catalog input. `build.py` combines it with the public API constants, pinned repository trees and the earlier announcement ledger, and produces the HTML/Markdown/CSV/JSON views. It reuses captured evidence by default. `python3 build.py --refresh` deliberately refreshes public documentation; it does not silently change the pinned source baselines. Reproduction requires the sibling `telegram-research-2026-10-20` package.

`python3 validate.py` checks identifiers, counts, all source hashes, captured official feature URLs, code-tree matches, CSV/JSON consistency, historical references and the generated viewer's embedded data. `validation.json` records the result. These checks validate the research artifact, not Telegram or Zoen functionality.

Official starting points: [API index](https://core.telegram.org/api), [Premium API reference](https://core.telegram.org/api/premium), [Premium FAQ](https://telegram.org/faq_premium), [Business](https://core.telegram.org/api/business), [Bot features](https://core.telegram.org/bots/features), [embedded Apps](https://core.telegram.org/bots/webapps), [release evolution](https://telegram.org/evolution), [official source repositories](https://telegram.org/apps#source-code).
