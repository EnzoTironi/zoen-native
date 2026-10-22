#!/usr/bin/env python3
"""Typecheck the real integrated UIKit video/file owner, no declaration stubs."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

video = Path(__file__).resolve().parents[1]
ios = video.parent
presentation = ios / 'TelegramAttachmentPresentation'
output = Path(sys.argv[1]).resolve()
lottie = Path(sys.argv[2]).resolve()
output.mkdir(parents=True, exist_ok=False)
headers = output / 'Integration.h'
headers.write_text('#import "' + str(presentation / 'ZoenTelegramPresentationRuntime.h') + '"\n#import "' + str(video / 'Sources/ZoenTelegramVideoEditing.h') + '"\n')
sources = sorted(ios.glob('ZoenFilePreview*.swift')) + [ios / name for name in ['ZoenPDFEditCapture.swift', 'ZoenMediaEditCapture.swift', 'ZoenAttachmentVideoEditor.swift']]
sources += sorted(video.glob('*.swift')) + sorted(presentation.glob('*.swift'))
module_sources = [ios / 'ZoenCoreModule.swift'] + sorted(ios.glob('ZoenSession*.swift'))
sdk = subprocess.check_output(['xcrun', '--sdk', 'iphonesimulator', '--show-sdk-path'], text=True).strip()
commands = [
    ('objc', ['xcrun', 'clang', '-fsyntax-only', '-fobjc-arc', '-fmodules', '-target', 'arm64-apple-ios26.0-simulator', '-isysroot', sdk] + [str(p) for p in sorted((video / 'Sources').glob('*.m'))] + [str(presentation / 'ZoenTelegramPresentationRuntime.m')]),
    ('swift', ['xcrun', 'swiftc', '-typecheck', '-swift-version', '5', '-target', 'arm64-apple-ios26.0-simulator', '-sdk', sdk,
               '-I', str(lottie), '-Xcc', '-fmodule-map-file=' + str(lottie / 'Lottie.modulemap'), '-import-objc-header', str(headers)] + [str(p) for p in sources]),
]
commands += [('module-parse-' + p.stem, ['xcrun', 'swiftc', '-frontend', '-parse', str(p)]) for p in module_sources]
results = []
for name, command in commands:
    result = subprocess.run(command, capture_output=True, text=True, timeout=120)
    (output / (name + '.log')).write_text(result.stdout + result.stderr)
    results.append({'name': name, 'command': command, 'exit': result.returncode, 'diagnostics': bool(result.stdout or result.stderr)})
    (output / 'commands.json').write_text(json.dumps(results, indent=2) + '\n')
    assert result.returncode == 0, name + ' failed: ' + str(output / (name + '.log'))
inventory = [{'path': str(p), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in sources + module_sources]
result = {'status': 'PASS', 'commands': results, 'sdk': sdk, 'sourceInventory': inventory,
    'declarationStubs': False, 'actualLottie': str(lottie),
    'limits': ['UIKit video and real shared glass/ObjC runtime are SDK-typechecked; CoreModule and its same-actor session extensions are parser-only because Rust/Expo linkage is qualified by the main build.',
               'No Pods or main native build, simulator/device execution, gestures, keyboard or UI parity claim.']}
(output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'status': 'PASS', 'output': str(output), 'checks': len(results), 'declarationStubs': False}))
