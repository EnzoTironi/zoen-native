#!/usr/bin/env python3
"""Exercise actual AVFoundation exports on macOS, never an iOS UI parity claim."""
from pathlib import Path
import json
import shutil
import subprocess
import sys

source = Path(__file__).resolve().parents[1]
output = Path(sys.argv[1]).resolve()
output.mkdir(parents=True, exist_ok=False)
ffmpeg = shutil.which('ffmpeg')
assert ffmpeg, 'existing ffmpeg required; do not install a substitute'
def run(args, name):
    result = subprocess.run(args, capture_output=True, text=True, timeout=120)
    (output / (name + '.log')).write_text(result.stdout + result.stderr)
    assert result.returncode == 0, name + ' failed; inspect ' + str(output / (name + '.log'))
    return result.stdout
run([ffmpeg, '-v', 'error', '-nostdin', '-f', 'lavfi', '-i', 'testsrc=size=160x120:rate=30', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=44100', '-t', '3', '-c:v', 'libx264', '-threads', '1', '-pix_fmt', 'yuv420p', '-c:a', 'aac', str(output / 'source.mp4')], 'fixture')
run([ffmpeg, '-v', 'error', '-nostdin', '-f', 'lavfi', '-i', 'sine=frequency=220:sample_rate=44100', '-t', '3', '-c:a', 'aac', str(output / 'audio.m4a')], 'audio-fixture')
harness = output / 'Harness.swift'
harness.write_bytes((source / 'Verification' / 'RendererProof.swift.template').read_bytes())
sdk = subprocess.check_output(['xcrun', '--sdk', 'macosx', '--show-sdk-path'], text=True).strip()
binary = output / 'renderer-proof'
run(['xcrun', 'swiftc', '-parse-as-library', '-warnings-as-errors', '-swift-version', '5', '-target', 'arm64-apple-macos15.0', '-sdk', sdk,
     str(source / 'ZoenTelegramVideoFailure.swift'), str(source / 'ZoenTelegramVideoRenderer.swift'), str(harness), '-o', str(binary)], 'compile')
result = run([str(binary), str(output)], 'renderer')
value = json.loads(result)
def frame(path, seconds):
    return subprocess.check_output([ffmpeg, '-v', 'error', '-nostdin', '-ss', str(seconds), '-i', str(path), '-frames:v', '1', '-vf', 'scale=32:24', '-pix_fmt', 'rgb24', '-threads', '1', '-f', 'rawvideo', '-'], timeout=30)
first = frame(output / 'trim-audio.mp4', 0)
selected = frame(output / 'source.mp4', 0.7)
untrimmed = frame(output / 'source.mp4', 0)
assert len(first) == len(selected) == len(untrimmed) == 32 * 24 * 3
def error(a, b):
    return sum(abs(x - y) for x, y in zip(a, b)) / len(a)
selected_error, untrimmed_error = error(first, selected), error(first, untrimmed)
assert selected_error < untrimmed_error, 'export begins at wrong source frame'
value['passed'].append('independent decoded pixels identify selected source start frame')
value['count'] = len(value['passed'])
value['frameStartProof'] = {'selectedMeanAbsoluteError': selected_error, 'untrimmedMeanAbsoluteError': untrimmed_error}
(output / 'result.json').write_text(json.dumps(value, indent=2) + '\n')
print(json.dumps({'output': str(output), 'passed': value['count'], 'platform': value['platform'], 'isIOSUIProof': False}))
