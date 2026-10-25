# Managed touch history (#583), v607

Native QA604 applied an agent paragraph-style edit in the same Word editor, but
Undo was unreachable in both orientations. Managed mode hid the desktop title
bar, which contained upstream's only Quick Access Undo/Redo controls.

The managed editor now exposes two icon controls with 44×44-point targets and
localized AccessKit button labels. Availability comes from the actual Session
history plus the existing command policy. Touch invokes upstream `edit.undo` or
`edit.redo` through `WordApp::run`; no key injection or second history is added.
The desktop title bar stays hidden. A normal dirty human draft may Undo; an
agent proposal, host pause or historical freeze cannot bypass its policy.

## Executed evidence

Producer source `1e0287b7ea378d3e081efb06643ac7f4b432bf57`, prepared normal
browser tree `39a86461b2ac418d182dced3573a78353de8b1a0`, upstream
`d1b0bfc3f9dd83126b162a42b667facecb003b7f`. Receipts and retained artifacts:
`/Users/enzotironi/.codex/builds/zoen-word-managed-history-v607`.

- Controlled baseline fails `managed_history_control_missing` after the actual
  agent-model refresh. The same localized test passes on the successor; 627
  common source files are identical, with only the UI call sites changed and
  `managed_history.rs` added.
- Final host/actual WordApp batch: 42/42 passed, including real touch Undo after
  agent refresh, exact prior model, unchanged binding/document identity, new
  checkpoint against the current head, Redo, and empty/pending/frozen controls.
- Host all-target Clippy pedantic/performance with warnings denied passed;
  WASM browser all-target Clippy with warnings denied passed. No suppression.
- Normal release passed in 195.26 seconds. wasm-bindgen 0.2.129 produced the
  assets below, inside the existing 512 KiB JS / 48 MiB WASM bounds.
- Actual WebKit with the official bootstrap/session consumer passed both normal
  input and observed press-focus refusal (`denied=1`). Human text at v2 → real
  headless agent paragraph-format proposal at v3 → touch Undo/save v4 → touch
  Redo/save v5 → exact selected DOCX. Paragraphs and styles match the respective
  original/agent models, the actual canvas remains installed, and page/console
  errors are empty. Its canonical read/CAS port is synthetic; it is not a new
  native Core authorization or persistence claim.

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| wordcraft.js | 80,480 | `d2406403c3e08dee7fd77087c79740efb937fa7652e2a29550341f906e69d160` |
| wordcraft_bg.wasm | 46,665,221 | `e6d3d62dab284167e2dd1bf19de568420c89efb52128793d28550a10306f6c29` |
| wordcraft.html | 626,645 | `282924beb0bf97cc623ff7a401c18981f1cffbf8f9d330e2b74b3ac65f8e7d86` |

The fresh normal HTML equals the existing generated repository HTML. Default
availability remains off; the resource builder verifies actual sealed assets.
The previous WASIp2 worker is retained with its original v581 producer receipt
and SHA `3e41a135c1faeb042071cbeddd23f76652836f4c032252b104787a9c73a62370`.
It was not rebuilt using v607's changed recipe/test inputs. Fonts/notices retain
their prior verified provenance. No new ABI, commands, authority or signatures.

Preserved non-product trials: the initial candidate used a panel API absent in
the pinned egui; the first test queried English despite the system language;
Clippy then required code formatting in one doc comment. Their receipts remain
separate from the controlled localized behavior RED and final green results.

## Native acceptance

iPhone touch, software-keyboard stability, orientation and VoiceOver remain for
the normal combined native QA cohort. Desktop WebKit and headless AccessKit do
not establish those results. Closing a Session does not persist its in-memory
Undo stack: after a new open, create/apply a new edit in that same editor before
checking Undo. Accepted versions and immutable historical bytes remain intact.
No device, service, model, SDK, deployment or signing action was performed here.
