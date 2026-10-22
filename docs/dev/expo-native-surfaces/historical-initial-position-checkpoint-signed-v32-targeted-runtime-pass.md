[Native surface index](../expo-native-surfaces.md).

## Historical initial-position checkpoint — signed v32, targeted runtime pass

Root observed initial latest-message/composer overlap in v29 on both QA accounts.
Signed v30's first-positive-composer gate and v31's positive initial footer spacer
both left the controlled main chat at 97%, with the reply badge ending 0.932 below
the input's top 0.902. A settled 30-second v30 capture was unchanged. Native model and
visible/AX frames differed by 54 points; that difference and source ordering do not
establish the full cause. These remain historical failed attempts, with evidence in
`/private/tmp/zoen-message-polish-qa/v31/runtime.json` and their artifact records.

Historical v32 source `994ec452` admits one cancellable animation-frame native
nonanimated end request after the actual native footer, list load and content size are ready, only
on normal fresh main-chat entry. User drag, search, reveal/jump, reply/edit, owner or
scope changes and unmount skip or cancel it. There is no fixed timer; ongoing
Reanimated footer updates, keyboard ownership and message/search anchors remain.
Six admission/cancellation regressions extend the full app check to 245/245, with
TypeScript, Oxlint and Oxfmt passing
(`/private/tmp/zoen-v32-initial-native-end-app-check.log`). V32 reuses the exact v30
rebuilt native core with unchanged generated Swift ABI
(`/private/tmp/zoen-iphone-v32-core-verification.json`).

Signed v32 build, strict signature, stable 743-input verification, archive and
preserving isolated installation passed; installed executable/JavaScript match.
Manifest SHA256 is
`38b119b0e852645114b94726a690785efa1337ee72141fdedaaf74c7b166a038`;
exact hashes are under `apps/zoen/acceptance.json → previousControlledV32IphoneArtifact`, with
`/private/tmp/zoen-iphone-release-v32-verification.json` as build/install evidence.

Actual same-history main-chat fresh entry without a swipe reaches 100%. The final
View 1 reply badge at y 0.820/height 0.050 ends 0.870, above composer top 0.902: a 0.032
window gap, approximately 28 points. Root reviewed
`/var/folders/9b/t_cg13vn2fz15fy_n31pj6jm0000gn/T/simserver-pgAu2y/media/730184000-1792281825745.png`.
The exact history passed two fresh entries and a 30-second settled read, all 100%
without swiping. Named `fresh-entry.png`, `settled-entry.png` and
`second-fresh-entry.png`, plus `runtime.json`, are under
`/private/tmp/zoen-message-polish-qa/v32/`. This is targeted initial-layout acceptance;
broad long/tall content, keyboard, accessibility and comparative acceptance remain
pending.

Human E2B also received a preserving verified v32 archive after a fresh idle Chats
preflight. Installed executable/JavaScript and strict signature passed. Restart
retained four canonical Home cards/progress and the Bilingual QA conversation/history.
Fresh entry without swipe places the existing audio/receipt bottom 0.876 above input
top 0.897; scrollbar 99% initially and 100% settled with the native Menu open. Menu
opened/dismissed without selection, imports, sends or account mutations. Named
`human-home.png`, `human-fresh-entry.png` and `human-native-menu.png` accompany the
runtime record; delivery checks are in the verification JSON `human_installation`.
Earlier independent media proof below belongs to actual v29 under
`previousControlledV29IphoneArtifact`; those media were not resent in v32.
