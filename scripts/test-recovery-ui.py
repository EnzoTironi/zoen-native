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
        self.assertTrue('backupTurnOnPassword(' not in settings, 'Password activation must not be offered')
        self.assertTrue('RecoveryKeySetup' in settings, 'Recovery setup must be reachable')

    def test_android_replaces_password_activation(self):
        settings = source('android/app/src/main/java/xyz/tironi/zoen/ui/Settings.kt')
        self.assertTrue('backupTurnOnPassword(' not in settings, 'Password activation must not be offered')
        self.assertTrue('RecoveryKeySetup' in settings, 'Recovery setup must be reachable')

    def test_core_stages_key_before_network_and_has_key_only_restore(self):
        core = source('crates/roda-ffi/src/backup_api.rs')
        for api in ['pub fn backup_create_recovery_key(', 'pub async fn backup_enable_recovery_key(', 'pub async fn restore_backup_with_recovery_key(']:
            self.assertTrue(api in core, f'Missing staged/key-only API: {api}')

    def test_native_restore_entry_points(self):
        for path in ['apple/Shared/Features/OnboardingFlow.swift', 'android/app/src/main/java/xyz/tironi/zoen/ui/Onboarding.kt']:
            self.assertTrue('RecoveryKeyRestore' in source(path), f'Missing recovery entry in {path}')

    def test_web_guidance_does_not_collect_secrets(self):
        web = source('packages/zoen-ui/web/recovery.tsx')
        self.assertTrue('Recuperar minha conta' in web)
        self.assertTrue('<input' not in web, 'Never collect a recovery key in the browser')
        self.assertTrue('RecoveryHelp' in source('packages/zoen-ui/web/main.tsx'))

if __name__ == '__main__':
    unittest.main()
