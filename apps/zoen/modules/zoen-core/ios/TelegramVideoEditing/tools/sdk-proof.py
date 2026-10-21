#!/usr/bin/env python3
"""Real UIKit/ObjC protocol + Swift SDK checks; no substitute declarations."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
output = Path(sys.argv[1]).resolve()
output.mkdir(parents=True, exist_ok=False)
sdk = subprocess.check_output(['xcrun', '--sdk', 'iphonesimulator', '--show-sdk-path'], text=True).strip()
target = 'arm64-apple-ios26.0-simulator'
objc = sorted((root / 'Sources').glob('*.m'))
swift = sorted(root.glob('*.swift'))
commands = [
    ['xcrun', 'clang', '-fsyntax-only', '-Werror', '-fobjc-arc', '-fmodules', '-target', target, '-isysroot', sdk] + [str(p) for p in objc],
    ['xcrun', 'swiftc', '-typecheck', '-warnings-as-errors', '-swift-version', '5', '-target', target, '-sdk', sdk,
     '-import-objc-header', str(root / 'Sources' / 'ZoenTelegramVideoEditing.h')] + [str(p) for p in swift],
]
results = []
for name, command in zip(['objc', 'swift'], commands):
    value = subprocess.run(command, text=True, capture_output=True, timeout=120)
    (output / (name + '.log')).write_text(value.stdout + value.stderr)
    results.append({'name': name, 'command': command, 'exit': value.returncode, 'diagnostics': bool(value.stdout or value.stderr)})
    assert value.returncode == 0, name + ' SDK check failed: ' + str(output / (name + '.log'))
manifest = json.loads((root / 'asset-sources.json').read_text())
for asset in manifest['sources']:
    assert hashlib.sha256((root / asset['output']).read_bytes()).hexdigest() == asset['sha256'], asset['output'] + ' hash mismatch'
inventory = [{'path': str(p.relative_to(root)), 'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
             for p in sorted(root.rglob('*')) if p.is_file()]
result = {'target': target, 'sdk': sdk, 'commands': results, 'sourceInventory': inventory,
          'originalAssetsVerified': len(manifest['sources']), 'declarationStubs': False,
          'limits': ['Not a linked Expo/Pods build, iOS simulator/device execution or UI parity proof.']}
(output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'output': str(output), 'checks': len(results), 'diagnostics': any(x['diagnostics'] for x in results), 'declarationStubs': False}))
