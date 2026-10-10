#!/usr/bin/env python3
"""Keep UniFFI's JNA callback threads attached until their native thread exits.

JNA 5.18.1 shares its detach flag between nested callbacks:
https://github.com/java-native-access/jna/blob/5.18.1/native/callback.c
"""

from pathlib import Path
import re
import sys


CALLBACK = """internal object uniffiRustFutureContinuationCallbackImpl: UniffiRustFutureContinuationCallback {
"""
CALLBACK_INTERFACE = re.compile(
    r"^internal interface (\w+)\s*:\s*com\.sun\.jna\.Callback\s*\{", re.MULTILINE
)
CALLBACK_OBJECT = re.compile(
    r"^(?P<indent> *)internal object (?:`[^`]+`|\w+)\s*:\s*(?P<interface>\w+)\s*\{\n",
    re.MULTILINE,
)


def initializer(indent: str) -> str:
    lines = [
        "    init {",
        "        // A nested JNA call must not detach an outer Java callback frame.",
        "        // JNA's pthread cleanup detaches when the native thread exits.",
        "        com.sun.jna.Native.setCallbackThreadInitializer(",
        "            this,",
        '            com.sun.jna.CallbackThreadInitializer(true, false, "zoen-native-callback")',
        "        )",
        "    }",
        "",
    ]
    return "".join(indent + line + "\n" if line else "\n" for line in lines)


def fix_source(source: str) -> str:
    if source.count(CALLBACK) != 1:
        raise ValueError("Expected one UniFFI Rust future continuation callback")
    interfaces = set(CALLBACK_INTERFACE.findall(source))
    if "UniffiRustFutureContinuationCallback" not in interfaces:
        raise ValueError("Expected the generated JNA future callback interface")
    count = 0
    configured = set()

    def configure(match: re.Match) -> str:
        nonlocal count
        if match["interface"] not in interfaces:
            return match.group()
        count += 1
        configured.add(match["interface"])
        init = initializer(match["indent"])
        remainder = source[match.end():]
        if remainder.startswith(init):
            return match.group()
        if not remainder.startswith(match["indent"] + "    override fun callback("):
            raise ValueError("UniFFI callback template changed; review the Android JNA initializer")
        return match.group() + init

    fixed = CALLBACK_OBJECT.sub(configure, source)
    if not count:
        raise ValueError("No generated JNA callback objects found")
    # These completion callbacks point into Rust, rather than Kotlin objects.
    # The dropped-future callback is only emitted for async callback traits.
    required = {
        name for name in interfaces
        if not name.startswith("UniffiForeignFutureComplete")
        and name != "UniffiForeignFutureDroppedCallback"
    }
    if required - configured:
        raise ValueError("Unconfigured JNA callback interfaces: " + ", ".join(sorted(required - configured)))
    return fixed


def fix_directory(directory: Path) -> int:
    count = 0
    for path in directory.rglob("*.kt"):
        source = path.read_text(encoding="utf-8")
        if CALLBACK in source:
            fixed = fix_source(source)
            if fixed != source:
                path.write_text(fixed, encoding="utf-8")
            count += 1
    if not count:
        raise ValueError("No generated UniFFI Rust future callback found")
    return count


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: fix-android-uniffi-callbacks.py GENERATED_KOTLIN_DIRECTORY")
    try:
        count = fix_directory(Path(sys.argv[1]))
    except ValueError as error:
        raise SystemExit(str(error)) from error
    print(f"Android JNA thread initialization verified for {count} UniFFI binding(s).")
