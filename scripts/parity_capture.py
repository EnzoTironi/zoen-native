#!/usr/bin/env python3
"""Render current native sources; never import historical screenshot baselines."""
import argparse
from datetime import datetime
import json
import os
import plistlib
from pathlib import Path
import shutil
import subprocess
from parity_compare import KEYS

ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    subprocess.run(args, check=True, cwd=ROOT, **kwargs)


def export_ios(exported, output):
    """Resolve xcresulttool's attachment index, never guess attachment UUIDs."""
    index = json.loads((exported/'manifest.json').read_text())
    found = {}
    def visit(value):
        if isinstance(value, list):
            for item in value:
                visit(item)
        elif isinstance(value, dict):
            filename = value.get('exportedFileName')
            name = value.get('suggestedHumanReadableName', value.get('name', ''))
            if filename:
                for key in KEYS:
                    if name == 'ios-'+key or name.startswith(('ios-'+key+'.', 'ios-'+key+'_')):
                        if key in found:
                            raise ValueError(f'Duplicate iOS attachment: {key}')
                        found[key] = exported/filename
                if name in ('parity-fixture.json', 'parity-fixture') or name.startswith(('parity-fixture.json_', 'parity-fixture_')):
                    if 'manifest' in found and json.loads(found['manifest'].read_text()) != json.loads((exported/filename).read_text()):
                        raise ValueError('Conflicting native fixture metadata attachments')
                    found['manifest'] = exported/filename
            for item in value.values():
                visit(item)
    visit(index)
    for key in (*KEYS, 'manifest'):
        if key not in found:
            raise ValueError(f'Missing fresh iOS attachment: {key}')
        shutil.copyfile(found[key], output/('manifest.json' if key == 'manifest' else key+'.png'))


def ios_status_time():
    # New simctl runtimes require an ISO timestamp including milliseconds.
    # Simulators inherit the runner's time zone: encode local noon with its
    # offset so the status bar remains 12:00 on both UTC CI and local Macs.
    return datetime(2026, 1, 1, 12).astimezone().isoformat(timespec='milliseconds')


def build_ios_core():
    """Build the one FFI slice consumed by iOS Simulator parity tests."""
    profile = os.environ.get('PROFILE', 'release')
    directory = 'debug' if profile == 'dev' else profile
    target = 'aarch64-apple-ios-sim'
    sdk = subprocess.check_output(
        ['xcrun', '--sdk', 'iphonesimulator', '--show-sdk-path'],
        cwd=ROOT, text=True).strip()
    build_env = dict(os.environ, SDKROOT=sdk, IPHONEOS_DEPLOYMENT_TARGET='26.0')
    build_env.pop('MACOSX_DEPLOYMENT_TARGET', None)
    run('cargo', 'build', '-q', '-p', 'roda-ffi', '--profile', profile,
        '--target', target, env=build_env)

    bindings = ROOT/'build/bindings'
    shutil.rmtree(bindings, ignore_errors=True)
    headers = bindings/'headers'
    headers.mkdir(parents=True)
    library = ROOT/'target'/target/directory
    run('cargo', 'run', '-q', '-p', 'uniffi-bindgen', '--', 'generate',
        '--library', str(library/'libroda_ffi.dylib'), '--language', 'swift',
        '--out-dir', str(bindings))
    shutil.copyfile(bindings/'roda_ffiFFI.h', headers/'roda_ffiFFI.h')
    shutil.copyfile(bindings/'roda_ffiFFI.modulemap', headers/'module.modulemap')

    package = ROOT/'apple/Packages/RodaCore'
    framework = package/'RodaFFI.xcframework'
    shutil.rmtree(framework, ignore_errors=True)
    run('xcodebuild', '-create-xcframework',
        '-library', str(library/'libroda_ffi.a'), '-headers', str(headers),
        '-output', str(framework))
    shutil.copyfile(bindings/'roda_ffi.swift',
                    package/'Sources/RodaCore/RodaFFI.generated.swift')


def capture_ios(args, output):
    device = args.device
    run('xcrun', 'simctl', 'bootstatus', device, '-b')
    run('xcrun', 'simctl', 'status_bar', device, 'override', '--time', ios_status_time(), '--dataNetwork', 'wifi', '--wifiMode', 'active', '--wifiBars', '3', '--batteryState', 'charged', '--batteryLevel', '100')
    build_ios_core()
    run('xcodegen', 'generate', '--spec', 'apple/project.yml')
    result = output/'native.xcresult'
    env = dict(os.environ, TZ='UTC', SIMCTL_CHILD_TZ='UTC', SIMCTL_CHILD_ZOEN_PARITY_FIXTURE='native-parity-v1', SIMCTL_CHILD_ZOEN_PARITY_REVISION=args.revision)
    derived = (args.derived_data or ROOT/'.reason/parity-derived-ios').resolve()
    run('xcodebuild', 'build-for-testing', '-project', 'apple/Zoen.xcodeproj', '-scheme', 'ZoenUITests', '-destination', f'platform=iOS Simulator,id={device}', '-derivedDataPath', str(derived), 'CODE_SIGNING_ALLOWED=NO', env=env)
    specs = list((derived/'Build/Products').glob('*.xctestrun'))
    if len(specs) != 1:
        raise ValueError(f'Expected one native test run specification, found {len(specs)}')
    spec = specs[0]
    with spec.open('rb') as file:
        settings = plistlib.load(file)
    fixture_env = {'TZ': 'UTC', 'ZOEN_PARITY_FIXTURE': 'native-parity-v1', 'ZOEN_PARITY_REVISION': args.revision, 'ZOEN_PARITY_VIEWPORT': args.viewport}
    def configure(value):
        if isinstance(value, dict):
            if 'TestBundlePath' in value:
                value.setdefault('EnvironmentVariables', {}).update(fixture_env)
                value.setdefault('TestingEnvironmentVariables', {}).update(fixture_env)
            for child in list(value.values()):
                configure(child)
        elif isinstance(value, list):
            for child in value:
                configure(child)
    configure(settings)
    with spec.open('wb') as file:
        plistlib.dump(settings, file)
    run('xcodebuild', 'test-without-building', '-xctestrun', str(spec), '-destination', f'platform=iOS Simulator,id={device}', '-only-testing:ZoenUITests/AndroidParityCaptureTests/testLightScreens', '-only-testing:ZoenUITests/AndroidParityCaptureTests/testDarkScreens', '-resultBundlePath', str(result), env=env)
    exported = output/'attachments'
    run('xcrun', 'xcresulttool', 'export', 'attachments', '--path', str(result), '--output-path', str(exported))
    export_ios(exported, output)


def android_emulator_serial(requested=None):
    """Never deploy or clear application data on a connected physical phone."""
    if requested and not requested.startswith('emulator-'):
        raise ValueError('Android parity capture only supports emulator serials')
    listing = subprocess.check_output(['adb', 'devices'], cwd=ROOT, text=True)
    online = [parts[0] for line in listing.splitlines()
              if len(parts := line.split()) == 2 and parts[1] == 'device'
              and parts[0].startswith('emulator-')]
    if requested:
        if requested not in online:
            raise ValueError(f'Requested Android emulator is not online: {requested}')
        return requested
    if len(online) != 1:
        raise ValueError('Supply --device with one online Android emulator serial')
    return online[0]


def validate_instrumentation_output(output):
    # `am instrument` can exit zero even when its runner or a test failed.
    failures = ('failures!!!', 'instrumentation_failed', 'process crashed', 'shortmsg=')
    if 'OK (' not in output or any(marker in output.lower() for marker in failures):
        raise ValueError('Android instrumentation did not report a successful test run')


def android_capture_test_classes():
    return ','.join('xyz.tironi.zoen.' +
        ('AndroidParityComposerCaptureTest' if key.startswith('composer-') else 'AndroidParityScreensTest') +
        '#' + key.replace('-light', 'Light').replace('-dark', 'Dark') for key in KEYS)


def set_android_timezone(adb, timezone='UTC'):
    # Writing persist.sys.timezone directly requires root on current emulator
    # images. AlarmManager exposes a shell command backed by SET_TIME_ZONE.
    run(*adb, 'shell', 'cmd', 'alarm', 'set-timezone', timezone)


def capture_android(args, output):
    serial = android_emulator_serial(args.device)
    adb = ('adb', '-s', serial)
    abi = subprocess.check_output([*adb, 'shell', 'getprop', 'ro.product.cpu.abi'], cwd=ROOT, text=True).strip()
    if abi not in ('arm64-v8a', 'x86_64'):
        raise ValueError(f'Unsupported emulator ABI: {abi}')
    run(*adb, 'shell', 'wm', 'size', '1206x2580')
    run(*adb, 'shell', 'wm', 'density', '480')
    run(*adb, 'shell', 'settings', 'put', 'system', 'font_scale', '1.0')
    run(*adb, 'shell', 'settings', 'put', 'global', 'window_animation_scale', '0')
    run(*adb, 'shell', 'settings', 'put', 'global', 'transition_animation_scale', '0')
    run(*adb, 'shell', 'settings', 'put', 'global', 'animator_duration_scale', '0')
    set_android_timezone(adb)
    run('bash', 'android/gradlew', '-p', 'android', ':app:assembleDebug', ':app:assembleDebugAndroidTest', '--max-workers=2', f'-PandroidAbis={abi}', f'-PcoreProfile={args.core_profile}')
    run(*adb, 'install', '-r', 'android/app/build/outputs/apk/debug/app-debug.apk')
    run(*adb, 'install', '-r', 'android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk')
    if args.clear_installed:
        run(*adb, 'shell', 'pm', 'clear', 'xyz.tironi.zoen')
    run(*adb, 'shell', 'run-as', 'xyz.tironi.zoen', 'rm', '-rf', 'cache/device-evidence/android-parity')
    result = subprocess.run([*adb, 'shell', 'am', 'instrument', '-w', '-r',
        '-e', 'class', android_capture_test_classes(),
        '-e', 'parityFixture', 'native-parity-v1', '-e', 'parityRevision', args.revision,
        'xyz.tironi.zoen.test/androidx.test.runner.AndroidJUnitRunner'], cwd=ROOT,
        text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=True)
    (output/'instrumentation.log').write_text(result.stdout)
    print(result.stdout)
    validate_instrumentation_output(result.stdout)
    # Test app owns these files; run-as only reads its debug sandbox after testing.
    for key in (*KEYS, 'manifest'):
        filename = 'manifest.json' if key == 'manifest' else key+'.png'
        with (output/filename).open('wb') as file:
            run(*adb, 'exec-out', 'run-as', 'xyz.tironi.zoen', 'cat', 'cache/device-evidence/android-parity/'+filename, stdout=file)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('platform', choices=('ios', 'android'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--derived-data', type=Path, help='Reuse current-source Xcode build products across iPhone/iPad captures')
    parser.add_argument('--device', help='Booted iOS simulator UDID or online Android emulator serial')
    parser.add_argument('--core-profile', choices=('dev', 'release'), default='release', help='Android JNI build profile')
    parser.add_argument('--viewport', choices=('narrow', 'wide'), default='narrow')
    parser.add_argument('--clear-installed', action='store_true')
    args = parser.parse_args()
    args.output = args.output.resolve()
    if args.output.exists():
        parser.error('Output must not exist: stale captures are not accepted')
    args.output.mkdir(parents=True)
    args.revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if args.platform == 'ios' and not args.device:
        parser.error('--device is required for iOS')
    (capture_ios if args.platform == 'ios' else capture_android)(args, args.output)
    # Native hooks, not this orchestrator, attest that the fixture was actually applied.
    manifest = json.loads((args.output/'manifest.json').read_text())
    if manifest.get('source_revision') != args.revision:
        raise ValueError('Native capture source revision does not match current checkout')

if __name__ == '__main__':
    main()
