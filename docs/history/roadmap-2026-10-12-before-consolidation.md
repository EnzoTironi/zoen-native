# Documentation before the Core193/native consolidation

Captured on 12 October 2026 at source `c21b7f8`. These documents contained older progress statements, including the 17:15 UTC snapshot. Their completion states are historical; the [current roadmap](../roadmap-status.md) supersedes them. Git preserves the complete original contents and failures.

| Original document | SHA-256 |
| --- | --- |
| [README.md](https://github.com/EnzoTironi/zoen-native/blob/c21b7f85f47c8a30d90120fff8b34259101df7ea/README.md) | `3c0817013b6159eb1e3ff1134f3cb71cfffb9c9edba66cad49b9cb60a6dec41e` |
| [docs/roadmap-status.md](https://github.com/EnzoTironi/zoen-native/blob/c21b7f85f47c8a30d90120fff8b34259101df7ea/docs/roadmap-status.md) | `8555cecd1dbed39919a6731766b65dee75b0773b73d7bb4ecfa24e8602db00ff` |
| [docs/product/live-pages.md](https://github.com/EnzoTironi/zoen-native/blob/c21b7f85f47c8a30d90120fff8b34259101df7ea/docs/product/live-pages.md) | `c45c6da95220d56ef9232a809159a53757be1f826438c2d6cf2e2f24f7c7ead2` |
| [docs/dev/live-page-session.md](https://github.com/EnzoTironi/zoen-native/blob/c21b7f85f47c8a30d90120fff8b34259101df7ea/docs/dev/live-page-session.md) | `e8de5bdf34694b2a0017428669821979a3492bf71816a9586b94a6438dad3933` |
| [docs/product/hig-compliance.md](https://github.com/EnzoTironi/zoen-native/blob/c21b7f85f47c8a30d90120fff8b34259101df7ea/docs/product/hig-compliance.md) | `9829f81918f42c26eaee23290c897410e0083108f5e3770e75ae9c833823bba9` |
| [docs/agent-runtime-capabilities.md](https://github.com/EnzoTironi/zoen-native/blob/c21b7f85f47c8a30d90120fff8b34259101df7ea/docs/agent-runtime-capabilities.md) | `ffd17073adba231e2c3d181259afbd65e3142ae60b11853cd5fa868caee1ad9f` |

PR63 incorporates the earlier backend source; PR66 subsequently adds offline typed Plan compatibility and validated damaged-media cache repair. The earlier 191/2 and 192/1 FFI failures retain their fixture/cache causes; the corrected full suite passes all193. Native builds v11–v14 exposed different Escape-routing failures; the v15 window-scoped implementation has current runtime evidence. Report-encoding failures remain utility negatives and do not alter app/Core results.

The earlier [11 October snapshot](roadmap-2026-10-11-0610.md) and [9 October audit](roadmap-2026-10-09.md) retain their original source and validation boundaries.
