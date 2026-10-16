# State-keyed icon parity

The main-screen parity gate remains mandatory. `scripts/parity_icon_compare.py`
adds a separate full-frame icon-state comparison; it does not replace any of the
16 main-screen captures or prove timing from static images alone.

## Producer contract

Use the same applied native fixture and source revision as the main capture.
Produce 40 PNGs: the events below, each in light/dark and full/reduced motion.

- `tab-chats-tap`, `tab-spaces-tap`, `tab-notifications-tap`, `tab-profile-tap`
- `composer-mic`, `composer-send`, `bell-new`
- `typing-active`, `connecting-active`, `icons-draw-on`

Filename: `<event>-<light|dark>-<full|reduced>.png`.
The directory's `manifest.json` contains the usual fixture fields and an
`icon_states` object keyed by the same filename without `.png`. Each entry has:

```json
{
  "event": "composer-send",
  "appearance": "light",
  "reduce_motion": false,
  "phase_ms": 160,
  "observed_state": {"draft": "Hi", "glyph": "arrow.up"}
}
```

The values must come from the actual applied application state. Never toggle an
animation counter directly to imitate a new notification. Do not manufacture a
successful recording or network event. Use the real input/selection handlers and
recorded fixture data through the same model update path as production.

Capture phases must match across platforms. Do not relabel a late frame or select
a different phase because its pixels look closer. A missing state, reduced-motion
mismatch, empty witness, phase mismatch, or missing PNG is a contract failure.
Only the main gate's OS-bar crop and full-frame size normalization apply. No icon
masks, content alignment, fuzzy colors, or per-event threshold increases.

```sh
python3 scripts/parity_icon_compare.py --ios parity/ios-icons \
  --android parity/android-icons --config scripts/parity_config.json \
  --output parity/report/icons-android --threshold 2.5
```

The same command accepts `--web` instead of `--android` for browser replicas.
Keep real, short MP4 recordings alongside the frames to inspect timing, easing,
interruptions, repeat-selected-tab taps, and Reduce Motion. A passing static frame
comparison does not prove those behaviors or physical haptics.

## Verification status

Contract unit tests exercise missing states/files, wrong motion mode, mismatched
application state and unaligned phases. They are not runtime motion evidence.
Native/Android producers, all 40 actual frame pairs, and recordings must be run
and inspected before claiming icon-motion parity.
