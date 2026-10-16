#!/usr/bin/env python3
import copy
import json
from pathlib import Path
import tempfile
import unittest

from parity_icon_compare import STATES, validate_states


class IconContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folders = [Path(self.temp.name)/name for name in ('ios', 'android')]
        self.states = {key: {**value, 'phase_ms': 160,
                            'observed_state': {'selection': value['event'], 'revision': 1}}
                       for key, value in STATES.items()}
        for folder in self.folders:
            folder.mkdir()
            for key in STATES:
                (folder/f'{key}.png').touch()
            self.write(folder, self.states)

    @staticmethod
    def write(folder, states):
        (folder/'manifest.json').write_text(json.dumps({'icon_states': states}))

    def test_requires_all_forty_matching_state_keys(self):
        self.assertEqual(len(validate_states(*self.folders)), 40)

    def test_missing_frame_is_not_a_pass(self):
        (self.folders[1]/f'{next(iter(STATES))}.png').unlink()
        with self.assertRaisesRegex(ValueError, 'Missing icon frame'):
            validate_states(*self.folders)

    def test_missing_state_is_not_a_pass(self):
        states = copy.deepcopy(self.states)
        del states[next(iter(STATES))]
        self.write(self.folders[1], states)
        with self.assertRaisesRegex(ValueError, 'Missing actual icon state'):
            validate_states(*self.folders)

    def test_reduced_motion_must_be_observed_not_only_named(self):
        states = copy.deepcopy(self.states)
        key = next(key for key in states if key.endswith('-reduced'))
        states[key]['reduce_motion'] = False
        self.write(self.folders[1], states)
        with self.assertRaisesRegex(ValueError, 'motion mismatch'):
            validate_states(*self.folders)

    def test_different_application_state_is_rejected(self):
        states = copy.deepcopy(self.states)
        states[next(iter(STATES))]['observed_state']['revision'] = 2
        self.write(self.folders[1], states)
        with self.assertRaisesRegex(ValueError, 'state or event phase differs'):
            validate_states(*self.folders)

    def test_unaligned_event_phases_are_rejected(self):
        states = copy.deepcopy(self.states)
        states[next(iter(STATES))]['phase_ms'] = 300
        self.write(self.folders[1], states)
        with self.assertRaisesRegex(ValueError, 'state or event phase differs'):
            validate_states(*self.folders)

    def test_invalid_phases_are_rejected(self):
        for phase in (True, -1, float('nan'), float('inf'), 10001, 10**1000):
            with self.subTest(phase=str(phase)):
                states = copy.deepcopy(self.states)
                states[next(iter(STATES))]['phase_ms'] = phase
                for folder in self.folders:
                    self.write(folder, states)
                with self.assertRaisesRegex(ValueError, 'event phase'):
                    validate_states(*self.folders)

    def test_no_state_witness_is_rejected(self):
        states = copy.deepcopy(self.states)
        states[next(iter(STATES))]['observed_state'] = {}
        self.write(self.folders[1], states)
        with self.assertRaisesRegex(ValueError, 'application-state witness'):
            validate_states(*self.folders)


if __name__ == '__main__':
    unittest.main()
