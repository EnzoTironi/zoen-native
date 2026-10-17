#!/usr/bin/env python3
"""Shared Apple icon source contracts only; not runtime SF rendering evidence."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
NAV = (ROOT / 'apple/Shared/Navigation/NativeNavigationSymbol.swift').read_text()
SHELL = (ROOT / 'apple/Shared/Navigation/ZoenAdaptiveShell.swift').read_text()
SETTINGS = (ROOT / 'apple/Shared/Features/Settings/SettingsKit.swift').read_text()


class SharedNavigationMotion(unittest.TestCase):
    def test_pending_identity_events_not_counts(self):
        self.assertIn('var notificationIDs: Set<String> = []', NAV)
        self.assertIn('.onChange(of: notificationIDs)', NAV)
        self.assertIn('if !new.subtracting(old).isEmpty', NAV)
        self.assertNotIn('notificationCount', NAV + SHELL)
        self.assertIn('Set(model.requests.filter { $0.status == .pending }.map(\\.id))', SHELL)
        # Same-count replacement signals new work; removals and unchanged sets do not.
        self.assertEqual({'b', 'c'} - {'a', 'b'}, {'c'})
        self.assertFalse({'a'} - {'a', 'b'})
        self.assertFalse({'a', 'b'} - {'a', 'b'})

    def test_selection_initial_state_is_silent(self):
        self.assertIn('.onChange(of: selected)', NAV)
        self.assertIn('if new && tap == 0 { selectionTap += 1 }', NAV)
        # Explicit tap counters already submit feedback; selection must not double-bounce.
        self.assertNotIn('initial: true', NAV)

    def test_toolbar_effects_are_on_real_symbols_with_action_events(self):
        self.assertIn('Button { sidebarTap += 1; setSidebar(!sidebarExpanded) }', SHELL)
        self.assertIn('NativeIconMotion(systemName: "sidebar.left", bounce: sidebarTap, drawOn: true)', SHELL)
        self.assertIn('Button { newMessageTap += 1; model.newChatOpen = true }', SHELL)
        self.assertIn('NativeIconMotion(systemName: "magnifyingglass", bounce: newMessageTap, drawOn: true)', SHELL)
        self.assertIn('.accessibilityLabel("Search")', SHELL)
        self.assertIn('Button { activation += 1; action() }', SETTINGS)
        self.assertIn('NativeIconMotion(systemName: symbol, bounce: activation, drawOn: true)', SETTINGS)

    def test_settings_press_respects_both_reduce_motion_preferences(self):
        press = SETTINGS.split('struct PressScale: ButtonStyle {', 1)[1].split('// MARK:', 1)[0]
        self.assertIn('@Environment(\\.nativeIconReduceMotion)', press)
        self.assertIn('systemReduceMotion || appReduceMotion', press)
        self.assertIn('enabled && !reduceMotion', press)
        self.assertIn('enabled && reduceMotion', press)


if __name__ == '__main__':
    unittest.main()
