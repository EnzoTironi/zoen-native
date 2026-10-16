# Zoen naming

The product, built-in agent and every code identifier are Zoen. The former name was removed in one prelaunch cutover (PR #126) without aliases, migrations or dual reads, per the [prelaunch policy](prelaunch-policy.md).

| Area | Current name |
| --- | --- |
| Rust crates and imports | `crates/zoen-*`, `zoen_*` |
| Bindings | `ZoenCore`, `ZoenFFI`, `ZoenEngine`, `zoen_ffi` |
| Apple preferences and launch flags | `Zoen…` (e.g. `-ZoenFreshStart`, `-ZoenAccount`, `-ZoenRelay`) |
| Local database | `Application Support/Zoen/zoen.sqlite` |
| MCP app resources | `ui://zoen/<app>` |
| Invite links | `https://tryzoen.com/c/<space>#<grant>` |

Development installs from before the cutover keep stale preferences and an orphaned `Roda/` folder; delete the app or reset the simulator. The original pet and ink artwork are hash-gated against their pre-rename source; `scripts/art-rename-equivalence.json` lists the only lines allowed to differ (Roda to Zoen renames). Historical documents under `docs/history` and ADRs keep the old name.
