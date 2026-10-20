# Telegram iOS as the primary UI reference

**User direction, 20 October 2026:** use Telegram iOS as the main interface and interaction reference when implementing Zoen. The current delivery is the Telegram feature matrix; selecting or implementing features follows later.

Start UI study with the official [Telegram iOS repository](https://github.com/TelegramMessenger/Telegram-iOS/tree/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838) and the [feature matrix](../../output/telegram-feature-matrix-2026-10-20/index.html). The matrix links pinned iOS entrypoints for each area. Android remains a secondary source for broader feature coverage and platform differences.

Apply this reference to conversation layout, composition and keyboard coordination, attachment selection/review, media viewing and playback, message actions/reactions, gestures, transitions, haptics, navigation, profiles and settings. Study the complete interaction, including interruption, cancellation, accessibility and recovery, rather than reproducing only a screenshot.

Keep Zoen's identity and accepted product requirements: its sky appearance, consistent App cards, full-screen Apps, document experiences, agents, permissions and backend contracts. For a selected feature, decide whether to reproduce the behavior with existing components or adapt a suitable upstream implementation after inspecting its dependencies and license. This reference decision alone does not select a Telegram fork, TDLib, Telegram infrastructure, or a different native framework.

Signal remains a supporting reference for reliability and behavioral tests. Notion and ChatGPT Spaces remain references for live documents and creation workflows. Telegram iOS leads the messaging UI direction.

The catalog and source references are research evidence. No implementation parity or native runtime validation is claimed by this decision.
