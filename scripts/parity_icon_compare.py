#!/usr/bin/env python3
"""Compare real, state-keyed icon frames, independently of the main-screen gate."""
import argparse
import json
import math
from pathlib import Path

from parity_compare import compare

EVENTS = ('tab-chats-tap', 'tab-marketplace-tap', 'tab-profile-tap',
          'composer-mic', 'composer-send', 'bell-new', 'typing-active',
          'connecting-active', 'icons-draw-on')
STATES = {f'{event}-{appearance}-{motion}':
          {'event': event, 'appearance': appearance, 'reduce_motion': motion == 'reduced'}
          for event in EVENTS for appearance in ('light', 'dark')
          for motion in ('full', 'reduced')}


def validate_states(reference, candidate):
    """Never compare differently keyed states or silently drop missing frames."""
    manifests = [json.loads((folder/'manifest.json').read_text())
                 for folder in (reference, candidate)]
    for key, expected in STATES.items():
        descriptions = []
        for folder, manifest in zip((reference, candidate), manifests):
            observed = manifest.get('icon_states', {}).get(key)
            if not isinstance(observed, dict):
                raise ValueError(f'Missing actual icon state: {key}')
            if any(type(observed.get(field)) is not type(value)
                   or observed.get(field) != value for field, value in expected.items()):
                raise ValueError(f'Icon event/appearance/motion mismatch: {key}')
            phase = observed.get('phase_ms')
            if (isinstance(phase, bool) or not isinstance(phase, (int, float))
                    or phase < 0 or phase > 10000 or not math.isfinite(phase)):
                raise ValueError(f'Missing measured event phase: {key}')
            state = observed.get('observed_state')
            if not isinstance(state, dict) or not state:
                raise ValueError(f'Missing actual application-state witness: {key}')
            if not (folder/f'{key}.png').is_file():
                raise ValueError(f'Missing icon frame: {key}')
            descriptions.append(observed)
        if descriptions[0] != descriptions[1]:
            raise ValueError(f'Icon state or event phase differs: {key}')
    return tuple(STATES)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('ios', 'config', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    targets = parser.add_mutually_exclusive_group(required=True)
    targets.add_argument('--android', type=Path)
    targets.add_argument('--web', type=Path)
    parser.add_argument('--threshold', type=float, default=2.5)
    args = parser.parse_args()
    candidate = args.android or args.web
    try:
        keys = validate_states(args.ios, candidate)
        passed = compare(args.ios, candidate, args.output,
                         json.loads(args.config.read_text()), args.threshold,
                         'android' if args.android else 'web', keys=keys)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(2, f'Icon capture contract failed: {error}\n')
    raise SystemExit(0 if passed else 1)


if __name__ == '__main__':
    main()
