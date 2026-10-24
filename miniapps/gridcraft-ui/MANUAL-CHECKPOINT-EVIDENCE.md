# Manual checkpoint input retention — #546

The compact producer previously froze `SheetApp` during every ordinary autosave.
A real egui keyboard regression on the unchanged producer failed after A1=7 →
snapshot → A2=9/Enter: A2 was empty. The original XLSX proposal must stay immutable,
but newer manual input and Undo must remain in the live Session. Only its exact
ACK advances the accepted baseline. Agent proposals and explicit pause still
freeze input. No debounce, authority, storage or document profile is added.

The producer qualification uses authored source `ec12b49ae646dd76418fbdf4c8ce9bc44abc825a`
and prepared upstream `47e7801ef0e76a5affac961a2ac4c9938baac0b8`, based on the
same pinned Grid engine and compact adaptation. Later consumer tests, package pins
and this evidence do not change any of the 33 compiled authored inputs. Normal
public preparation replayed all 559 crates/apps/vendor/root-manifest files exactly;
sparse upstream documentation is outside that comparison. Cargo generated the
actual lockfile; its SHA-256 is
`25c30a23211cf50aa123753c4aaa3fac1e5f47d4837624a471753cd848de9a39`.

Receipts retained under the external build directory
`zoen-grid-manual-ack-v505`:

- `red-01`: original **0 PASS / 1 FAIL**, A2 Empty versus Number(9). Actual log
  `be9bbfabb4e005746be8b66ebeb729e720bb5c5d3563b82d0b0c0a735a572331`.
- `check-01`: first successor **33 PASS / 1 FAIL**. New retention regression passed;
  an old automatic-freeze assertion needed the explicit host pause it intended
  to test. Its original log remains
  `373256d8b2a0a745d7eb09fa466ae44225d433a5e90c28282e44cdbc35bae505`.
- `check-02`: host all-target Clippy `-D warnings` passed; **34 actual engine and
  SheetApp/input cases passed, zero ignored**. Includes original/retry XLSX bytes,
  late and wrong ACK, newer checkpoint/Undo, explicit pause, pending agent,
  composition, touch and cold accepted-byte reopen. Result SHA-256
  `e08ccc95109c8847f5b632adcd3e0c57601aa22ae1fced915311100bdd892bb5`.
- `wasm-01`: actual wasm32 all-target Clippy and Release build passed, 12.159s and
  118.635s respectively. Result
  `f17ab8a93308edcf483efcc9274cf8f4329bb8dd9e7f7329b573ecde040ba5bf`.
- `browser-01`: a new initial-click setup did not focus the keyboard target;
  stopped before the edit. `browser-02`: the test paused the host before queued
  browser input had reached its animation frame, so A2 was still absent. Both
  failures remain. The final test uses the existing keyboard-focus path and a
  normal frame barrier before explicit pause; trusted touch is covered separately
  by the unchanged retired-agent consumer, not inferred from keyboard focus.
- `browser-03`: actual opaque WASM ABI consumer passed. Original XLSX has A1=7
  and no A2; newer accepted XLSX has A2=9; subsequent agent formula exports 14.
  Wrong/late ACK, explicit pause, exact reopen, rotation, forbidden external
  formula, retirement and actual retired-agent touch checks remain. Receipt
  `83a02906acee464aa84db84e7b3f12208d35c789df9a55a4639d3c73f3484831`.
- `browser-mcp-01`: setup supplied Deck HTML to Grid and timed out without opening;
  preserved. `browser-mcp-02`: the actual generated Grid MCP bootstrap, shared
  session/autosave/export and real engine passed. First ordinary save is held
  while real keys enter A2=9; two distinct saves reach head 3. Close and a fresh
  renderer reopen exact accepted XLSX with A1=7/A2=9 and zero new writes. Receipt
  `34b027145acb5d91f54618b48bd53d1251a4710a3b8df888bdbd4f1b6e312527`.

All successful runners report empty cleanup errors. The browser CAS/receipts are
synthetic component ports. They do not prove Rust authority, relay delivery,
physical persistence, iPhone focus or an actual native user journey. Keystrokes
only in WebView memory are not claimed durable.

The new immutable package uses an undeployed `manual505/` URL namespace. Existing
sealed packages and service files remain untouched. JS is unchanged:
78,601 bytes, `c79d78db09183721f5c6c5554f0e7a3b92457b5a6455c1162125bab82ba0c83d`.
New wasm-bindgen 0.2.129 output is 17,121,603 bytes,
`916c894df62ffbe0d3313f95e452a6d6fb9a371b0c0c15aef9929bbaa83f43cc`.
The exact generated HTML used in the MCP consumer is 623,816 bytes,
`ed3ef4c79572ddc78b87bacc1924090cf0438ba0494fab0bca198329762ab183`.
A later combined bootstrap must generate its own exact manifest. Default App
availability remains false. The new pin and retired-old-package regression still
require the consuming Miniapps compiler/test cohort; native acceptance remains
pending. Neither issue #546 nor the broader Grid journey closes on this evidence.
