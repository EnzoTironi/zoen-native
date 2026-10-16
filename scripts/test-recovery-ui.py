#!/usr/bin/env python3
"""Source wiring guards, not native UI/runtime verification."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent.parent

def source(path):
    return (ROOT / path).read_text()

class RecoveryWiring(unittest.TestCase):
    def test_apple_replaces_password_activation(self):
        settings = source('apple/Shared/Features/Settings/SettingsScreens.swift')
        self.assertNotIn('backupTurnOnPassword(', settings)
        self.assertIn('RecoveryKeySetup', settings)

    def test_android_replaces_password_activation(self):
        settings = source('android/app/src/main/java/xyz/tironi/zoen/ui/Settings.kt')
        self.assertNotIn('backupTurnOnPassword(', settings)
        self.assertIn('RecoveryKeySetup', settings)

    def test_core_stages_key_before_network_and_has_key_only_restore(self):
        core = source('crates/roda-ffi/src/backup_api.rs')
        self.assertIn('pub fn backup_create_recovery_key(', core)
        self.assertIn('pub async fn backup_enable_recovery_key(', core)
        self.assertIn('pub async fn restore_backup_with_recovery_key(', core)

    def test_native_restore_entry_points(self):
        self.assertIn('RecoveryKeyRestore', source('apple/Shared/Features/OnboardingFlow.swift'))
        self.assertIn('RecoveryKeyRestore', source('android/app/src/main/java/xyz/tironi/zoen/ui/Onboarding.kt'))

    def test_web_guidance_does_not_collect_secrets(self):
        web = source('packages/zoen-ui/web/recovery.tsx')
        self.assertIn('Recuperar minha conta', web)
        self.assertNotIn('<input', web)
        self.assertIn('RecoveryHelp', source('packages/zoen-ui/web/main.tsx'))

if __name__ == '__main__':
    unittest.main()
