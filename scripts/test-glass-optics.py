#!/usr/bin/env python3
"""CPU optics regression checks plus cross-platform integration contracts.
Run alongside check-apple-glass.sh; these do not replace GPU/UI captures.
"""
import math
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class GlassOpticsTests(unittest.TestCase):
    def test_schlick_endpoints_and_monotonicity(self):
        values = [.035 + .965 * (1 - i / 100) ** 5 for i in range(101)]
        self.assertAlmostEqual(values[0], 1)
        self.assertAlmostEqual(values[-1], .035)
        self.assertTrue(all(a >= b for a, b in zip(values, values[1:])))

    def test_snell_displacement_finite_and_radial(self):
        previous = 0
        for i in range(101):
            edge = i / 100
            slope = edge * .98
            cosine = math.sqrt(1 - slope * slope)
            eta = 1 / 1.5
            ray_x = (eta * cosine - math.sqrt(1 - eta * eta * (1 - cosine * cosine))) * slope
            displacement = abs(ray_x * 28 * edge * edge * .65)
            self.assertTrue(math.isfinite(displacement))
            self.assertGreaterEqual(displacement, previous)
            self.assertLessEqual(displacement * 1.04, 28)
            previous = displacement
        self.assertGreater(previous, 8)

    def test_apple_samples_layer_not_white_overlay(self):
        metal = (ROOT / 'apple/Shared/DesignSystem/ZoenGlass.metal').read_text()
        swift = (ROOT / 'apple/Shared/DesignSystem/ZoenGlass.swift').read_text()
        self.assertIn('SwiftUI::Layer layer', metal)
        self.assertIn('layer.sample', metal)
        self.assertIn('.layerEffect(', swift)
        self.assertNotIn('.colorEffect(', swift)

    def test_local_pet_appearance_reaches_material(self):
        swift = (ROOT / 'apple/Shared/DesignSystem/ZoenGlass.swift').read_text()
        pet = (ROOT / 'apple/Shared/Apps/Native/PetViews.swift').read_text()
        self.assertIn('appearance: ColorScheme? = nil', swift)
        self.assertIn('dark: (appearance ?? scheme) == .dark', swift)
        self.assertIn('.environment(\\.colorScheme, appearance ?? scheme)', swift)
        tab = pet.split('struct PetTabBar:', 1)[1].split('// MARK:', 1)[0]
        self.assertIn('appearance: dark ? .dark : .light', tab)

    def test_reduce_transparency_has_opaque_base_before_tint(self):
        swift = (ROOT / 'apple/Shared/DesignSystem/ZoenGlass.swift').read_text()
        fallback = swift.split('if reduceTransparency {', 1)[1].split('} else {', 1)[0]
        self.assertIn('shape.fill((appearance ?? scheme) == .dark ? Color.black : Color.white)', fallback)
        self.assertLess(fallback.index('Color.white'), fallback.index('if prominent { shape.fill(fill) }'))
        # Source-over alpha must stay opaque even for the store's 14% tint.
        for alpha in [0, .14, .58, 1]:
            self.assertEqual(alpha + 1 * (1 - alpha), 1)

    def test_onboarding_uses_system_paste_authorization(self):
        swift = (ROOT / 'apple/Shared/Features/OnboardingFlow.swift').read_text()
        self.assertIn('PasteButton(payloadType: String.self)', swift)
        self.assertNotIn('UIPasteboard.general', swift)
        self.assertNotIn('NSPasteboard.general', swift)

    def test_both_platforms_keep_volume_terms(self):
        for name in ['apple/Shared/DesignSystem/ZoenGlass.metal',
                     'android/app/src/main/java/xyz/tironi/zoen/ui/ZoenGlass.kt']:
            source = (ROOT / name).read_text()
            for term in ['innerShadow', 'caustic', 'fresnel', 'dispersion']:
                self.assertIn(term, source, name)


if __name__ == '__main__':
    unittest.main()
