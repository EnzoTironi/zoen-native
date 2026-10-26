# Local iPhone QA driver

Run local driver commands through the repository wrapper, with the verb first:

```sh
python3 scripts/qa-device.py snapshot -i --session <authorized-session>
python3 scripts/qa-device.py screenshot --session <authorized-session>
```

The wrapper refuses **all `fill` and `type` commands**, including empty and
nonempty text, before starting `agent-device`. Leading options are refused too;
put session/platform options after the verb. Other commands retain their literal
arguments, environment, working directory, streams and exit status. This does
not authorize any device, account, navigation or content action by itself.

For now, text changes require normal, verified selection/editing within an
explicitly authorized scope, or leave the new draft intact. Do not bypass this
guard through the raw CLI, batch/replay, MCP or another interface. A command
acknowledgment or an empty field does not prove that a draft was safely cleared:
inspect the actual resulting conversation and preserve unexpected messages.

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
