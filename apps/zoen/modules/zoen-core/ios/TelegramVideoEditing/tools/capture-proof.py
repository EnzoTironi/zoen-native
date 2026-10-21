#!/usr/bin/env python3
"""Run actual FilePreview/capture + AVFoundation host regressions, not iOS UI."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys

source = Path(__file__).resolve().parents[1]
ios = source.parent
core = ios.parent
output = Path(sys.argv[1]).resolve()
output.mkdir(parents=True, exist_ok=False)
ffmpeg = shutil.which('ffmpeg')
assert ffmpeg, 'existing ffmpeg required; do not install a substitute'
def run(command, name):
    result = subprocess.run(command, capture_output=True, text=True, timeout=120)
    (output / (name + '.log')).write_text(result.stdout + result.stderr)
    assert result.returncode == 0, name + ' failed: ' + str(output / (name + '.log'))
    return {'command': command, 'exit': result.returncode}
commands = []
commands.append(run([ffmpeg, '-v', 'error', '-nostdin', '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=30',
    '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=44100', '-t', '3', '-c:v', 'libx264',
    '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-threads', '2', str(output / 'source.mp4')], 'fixture'))
sdk = subprocess.check_output(['xcrun', '--sdk', 'macosx', '--show-sdk-path'], text=True).strip()
shared = [ios / name for name in ['ZoenFilePreview.swift', 'ZoenPDFEditCapture.swift', 'ZoenMediaEditCapture.swift']]
video = [source / 'ZoenTelegramVideoFailure.swift', source / 'ZoenTelegramVideoRenderer.swift']
for test, count in [('VideoEditTests', 7), ('MediaEditTests', 7), ('FilePreviewTests', 11), ('PDFEditTests', None)]:
    testSource = core / 'tests' / (test + '.swift')
    sources = shared + (video if test == 'VideoEditTests' else []) + [testSource]
    commands.append(run(['xcrun', 'swiftc', '-parse-as-library', '-warnings-as-errors', '-swift-version', '5',
        '-target', 'arm64-apple-macos15.0', '-sdk', sdk] + [str(p) for p in sources] + ['-o', str(output / test)], test + '-compile'))
    commands.append(run([str(output / test)] + ([str(output / 'source.mp4')] if test == 'VideoEditTests' else []), test))
inventory = [{'path': str(p), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in shared + video +
    [core / 'tests' / (name + '.swift') for name in ['VideoEditTests', 'MediaEditTests', 'FilePreviewTests', 'PDFEditTests']]]
result = {'status': 'PASS', 'platform': 'macOS host AVFoundation', 'sdk': sdk,
    'commands': commands, 'sourceInventory': inventory, 'newVideoScenarios': 7,
    'limits': ['Actual FilePreview/capture and renderer, synthetic presentation and save callbacks; no Rust CAS or iOS UIKit/Expo runtime proof.',
               'Existing image/PDF/FilePreview host suites run without production source substitution.']}
(output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'output': str(output), 'status': result['status'], 'suites': 4, 'newVideoScenarios': 7, 'isIOSUIProof': False}))
