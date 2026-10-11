#!/usr/bin/env python3
"""Check that binding regeneration cannot silently remove the native callback fix."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "android_callbacks", Path(__file__).with_name("fix-android-uniffi-callbacks.py")
)
callbacks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(callbacks)

ORIGINAL = """internal interface UniffiRustFutureContinuationCallback : com.sun.jna.Callback {
    fun callback(data: Long, pollResult: Byte)
}
""" + callbacks.CALLBACK + """    override fun callback(data: Long, pollResult: Byte) {
        uniffiContinuationHandleMap.remove(data).resume(pollResult)
    }
}
"""


class CallbackGenerationTest(unittest.TestCase):
    def test_regeneration_preserves_polling_and_configures_safe_thread_lifetime(self):
        fixed = callbacks.fix_source(ORIGINAL)
        self.assertIn('CallbackThreadInitializer(true, false, "zoen-native-callback")', fixed)
        self.assertIn("uniffiContinuationHandleMap.remove(data).resume(pollResult)", fixed)
        self.assertEqual(fixed, callbacks.fix_source(fixed))

    def test_changed_template_fails_instead_of_shipping_unconfigured_callbacks(self):
        with self.assertRaises(ValueError):
            callbacks.fix_source(ORIGINAL.replace("    override fun callback(", "    override fun changedCallback("))
        with self.assertRaises(ValueError):
            callbacks.fix_source(ORIGINAL + ORIGINAL)

    def test_directory_checks_nested_bindings_and_rejects_missing_generation(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            with self.assertRaises(ValueError):
                callbacks.fix_directory(directory)
            kotlin = directory / "xyz/tironi/zoen/core/roda_ffi.kt"
            kotlin.parent.mkdir(parents=True)
            kotlin.write_text(ORIGINAL)
            unrelated = directory / "Other.kt"
            unrelated.write_text("data class Other(val name: String)")
            self.assertEqual(1, callbacks.fix_directory(directory))
            self.assertEqual(callbacks.fix_source(ORIGINAL), kotlin.read_text())
            self.assertEqual("data class Other(val name: String)", unrelated.read_text())

    def test_listener_vault_and_free_callbacks_receive_the_same_thread_lifetime(self):
        extras = """internal interface UniffiCallbackInterfaceCoreListenerMethod0 : com.sun.jna.Callback {
    fun callback(handle: Long)
}
internal interface UniffiCallbackInterfaceSecretVaultMethod0 : com.sun.jna.Callback {
    fun callback(handle: Long)
}
internal interface UniffiCallbackInterfaceFree : com.sun.jna.Callback {
    fun callback(handle: Long)
}
internal object listener {
    internal object `onChange`: UniffiCallbackInterfaceCoreListenerMethod0 {
        override fun callback(handle: Long) { otherJnaCall() }
    }
    internal object `load`: UniffiCallbackInterfaceSecretVaultMethod0 {
        override fun callback(handle: Long) { otherJnaCall() }
    }
    internal object free: UniffiCallbackInterfaceFree {
        override fun callback(handle: Long) { otherJnaCall() }
    }
}
"""
        fixed = callbacks.fix_source(ORIGINAL + extras)
        self.assertEqual(4, fixed.count("Native.setCallbackThreadInitializer("))
        self.assertEqual(3, fixed.count("{ otherJnaCall() }"))
        self.assertEqual(fixed, callbacks.fix_source(fixed))

    def test_changed_callback_declaration_cannot_leave_an_interface_unprotected(self):
        source = ORIGINAL + """internal interface UniffiCallbackInterfaceCoreListenerMethod0 : com.sun.jna.Callback {
    fun callback(handle: Long)
}
internal class ChangedCallback: UniffiCallbackInterfaceCoreListenerMethod0 {
    override fun callback(handle: Long) { otherJnaCall() }
}
"""
        with self.assertRaises(ValueError):
            callbacks.fix_source(source)


if __name__ == "__main__":
    unittest.main()
