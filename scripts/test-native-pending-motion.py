#!/usr/bin/env python3
"""Source-contract regression, not a Swift runtime or UIKit effect test."""
from pathlib import Path
import unittest

SOURCE = (Path(__file__).resolve().parents[1] / 'apple/iOS/ZoeniOSApp.swift').read_text()

class PendingMotionContract(unittest.TestCase):
    def test_actual_pending_ids_feed_bridge(self):
        self.assertIn('Set(model.requests.filter { $0.status == .pending }.map(\\.id))', SOURCE)

    def test_initial_baseline_is_silent_and_replacements_are_events(self):
        self.assertIn('previousPending.map { pendingIDs.subtracting($0) } ?? []', SOURCE)
        self.assertIn('guard !added.isEmpty,', SOURCE)
        self.assertLess(SOURCE.index('previousPending = pendingIDs'), SOURCE.index('DispatchQueue.main.async'))
        self.assertNotIn('pendingCount > old', SOURCE)
        # Semantic examples of the subtraction contract, including unchanged total.
        self.assertFalse(set(['a', 'b', 'c']) - set(['a', 'b', 'c']))
        self.assertEqual(set(['a', 'b', 'd']) - set(['a', 'b', 'c']), {'d'})

    def test_driver_is_debug_and_fixture_gated(self):
        self.assertIn('#if DEBUG\n/// Explicit opt-in real-core event producer', SOURCE)
        self.assertIn('environment["ZOEN_PARITY_MOTION_DRIVER"] == "1"', SOURCE)
        self.assertIn('NativeParityClock.appliedManifest(core: model.core) != nil', SOURCE)
        self.assertIn('model.core.demoOpenRequest(', SOURCE)
        self.assertIn('action: "external", cents: 0)', SOURCE)
        self.assertIn('model.refresh()', SOURCE)

if __name__ == '__main__':
    unittest.main()
