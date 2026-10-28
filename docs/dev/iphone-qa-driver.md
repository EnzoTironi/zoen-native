# Local iPhone QA driver

Run local driver commands through the repository wrapper, with the verb first:

```sh
python3 scripts/qa-device.py snapshot -i --session <authorized-session>
python3 scripts/qa-device.py screenshot --session <authorized-session>
```

Update an existing installation with **`install <app> <binary>`**, preserving the
same app identity. Do not use `reinstall`: the qualified driver's iOS deployment
implementation uninstalls the app first, deleting its local database and drafts
while Keychain keys remain. The wrapper refuses `reinstall` and `uninstall` before
dispatch, including with a qualified private candidate. Do not bypass this through
batch, the raw driver, or another interface. Before an update, preserve a private
copy of the closed app's data; verify the account, drafts and messages afterwards.

QA764 used `reinstall` in error on 2026-10-28. The resulting account database is
empty and the app correctly refuses to replace the remaining Keychain keys. This
is a destructive QA operation, not proof of a product migration failure. Native
quote-motion acceptance remains pending; the previous QA762 evidence is historical.

The default wrapper refuses **all `fill` and `type` commands**, including empty and
nonempty text, before starting `agent-device`. Leading options are refused too;
put session/platform options after the verb. Other commands retain their literal
arguments, environment, working directory, streams and exit status. This does
not authorize any device, account, navigation or content action by itself.

Without the qualified private driver below, text changes require normal, verified selection/editing within an
explicitly authorized scope, or leave the new draft intact. Do not bypass this
guard through the raw CLI, batch/replay, MCP or another interface. A command
acknowledgment or an empty field does not prove that a draft was safely cleared:
inspect the actual resulting conversation and preserve unexpected messages.

## Qualified private driver

The external MIT candidate at exact clean commit
`ca3075064f7ba38b2cf452996556aa4b8b9db5cc` removes geometric caret taps and requires
observed target/focus/empty readback before replacement. Fixture696 passed all ten
isolated UIKit scenarios; fixture699 passed normal CLI empty fill, Unicode and
multiline replacement, and append, with exact resulting values and Send=0.
These are Simulator/tool proofs, not physical-device or whole-product acceptance.
The official0.21.31 version alone is insufficient; its unmodified clear is unsafe.

An explicitly scoped QA case may select this checkout through the same wrapper:

```sh
export ZOEN_QA_DRIVER_ROOT=<absolute-qualified-checkout>
export ZOEN_QA_NODE_BIN=<absolute-node-executable>
export AGENT_DEVICE_HOME=<private-config-directory>
export AGENT_DEVICE_STATE_DIR=<private-daemon-directory>
export AGENT_DEVICE_IOS_RUNNER_DERIVED_PATH=<private-derived-data-directory>
export AGENT_DEVICE_IOS_BUNDLE_ID=dev.agentdevice.zoenqa.v699
export AGENT_DEVICE_IOS_RUNNER_TEST_BUNDLE_ID=dev.agentdevice.zoenqa.v699.uitests
python3 scripts/qa-device.py snapshot -i --session <authorized-session>
```

Config, daemon and build directories must be distinct absolute paths. Other
connection configs are refused. Every command verifies the exact clean checkout;
an invalid candidate never falls back to the installed driver. Nothing installs
or upgrades the global CLI. Start with a fresh empty daemon directory: the wrapper
records its source and refuses an unknown existing daemon profile, even if it
reports the same upstream version. Keep session/device/target selection explicit and
verify resulting text and conversation state; approval of this tool does not
authorize a particular edit, Send, account or device action. Preserve the earlier
693 failures and CORE687 extra message; neither needs cleanup or replay.

This is an operational guard against an observed automation defect, **not a
sandbox or security boundary**. The raw CLI still exists. The wrapper does not
inspect nested scripts or establish safety for all other driver commands.

## Why text entry is blocked

[Issue #526](https://github.com/EnzoTironi/zoen-native/issues/526) already prohibited
empty-fill cleanup. CORE687 repeated it on installed `agent-device` **0.21.28**:

- `fill id="message-input" ''` became a runner `type` command in replacement mode.
- Installed `RunnerTests+TextTyping.swift:18–47` routes empty replacement through
  `clearTextInput`. `RunnerTests+TextEntry.swift:244–262,411–433` first moves the
  caret by tapping `(field.maxX - 4, field.midY)`, then types Delete characters.
- The retained field rectangle was `(54,478.333,340,59.667)`; enabled Send was
  `(348,498,46,40)`. The computed tap `(390,508.167)` is inside Send.
- The existing runner log records that tap at **06:55:08.582** on 2026-10-26
  (UTC−03), followed by Delete typing at **06:55:09.007**. Extra message
  `01M5VT57VPGC9AWHYF73X2C4P9` has timestamp **06:55:08.790**, between those events.
  No Return keystroke is shown. The product composer is multiline and its
  `onSubmitEditing` handler is search-only; its Send button uses `onPress`.

This evidence supports the driver's caret tap hitting Send; it is not an
independent trace of the app's callback. Source: installed package
`/opt/homebrew/lib/node_modules/agent-device/dist/apple/runner/AgentDeviceRunner/AgentDeviceRunnerUITests/`.
Retained QA evidence is in `output/iphone-qa-345/core687/` in the QA worktree:
`pasted-inspection-ax.json`, `clear-ONLY-new-inspection-draft.txt` and
`unexpected-extra-message-receipt.json`. No reproduction or cleanup is required.

Nonempty replacement with a resolved, occupied field also calls `clearTextInput`
(`RunnerTests+TextTyping.swift:121–138`). An alternate synthesized replacement
route exists; it does not make all fills safe. `type`/append can also enter this
clear path during automatic mismatch repair (`:483`), so it is blocked too.
Correcting the driver requires target-bound editing without the geometric caret
tap. Do not change the product's Send behavior to compensate for this driver bug.

Verify this wrapper without a driver/device:

```sh
python3 -m unittest discover -s scripts/tests -p test_qa_device.py
```
