#!/usr/bin/env python3
"""Compare fresh native captures; no baselines, masks or content alignment."""
import argparse
import json
from pathlib import Path
from PIL import Image, ImageChops

SCREENS = ('chats', 'chat', 'composer', 'spaces', 'notifications', 'profile', 'settings', 'onboarding')
KEYS = tuple(f'{screen}-{mode}' for screen in SCREENS for mode in ('light', 'dark'))
FIXTURE_FIELDS = ('fixture_id', 'fixed_time', 'locale', 'timezone', 'seed', 'source_revision')


def normalize(image, crop, size):
    """Only declared OS-bar crops and uniform full-frame size normalization."""
    left, top, right, bottom = crop
    if min(crop) < 0 or left + right >= image.width or top + bottom >= image.height:
        raise ValueError('Invalid OS bar crop')
    image = image.convert('RGB').crop((left, top, image.width-right, image.height-bottom))
    if image.size != tuple(size):
        # Reject aspect distortion instead of silently squeezing app controls.
        if abs(image.width / image.height - size[0] / size[1]) > 0.002:
            raise ValueError(f'Content aspect ratio differs: {image.size} versus {size}')
        image = image.resize(size, Image.Resampling.LANCZOS)
    return image


def compare(ios, candidate, output, config, threshold=2.5, platform='android', keys=KEYS):
    if not 0 <= threshold <= 100:
        raise ValueError('Threshold must be between 0 and 100')
    output.mkdir(parents=True, exist_ok=True)
    manifests = [json.loads((folder/'manifest.json').read_text()) for folder in (ios, candidate)]
    for field in FIXTURE_FIELDS:
        if field not in manifests[0] or manifests[0][field] != manifests[1].get(field):
            raise ValueError(f'Missing/mismatched native fixture field: {field}')
    expected = dict(fixture_id='native-parity-v1', fixed_time='2026-01-15T12:00:00Z', locale='en_US', timezone='UTC', seed=128)
    if any(manifests[0].get(key) != value for key, value in expected.items()):
        raise ValueError('Unsupported native fixture values')
    if not manifests[0]['source_revision']:
        raise ValueError('Missing native fixture source revision')
    results = []
    for key in keys:
        try:
            frames = []
            for source, folder in (('ios', ios), (platform, candidate)):
                with Image.open(folder/f'{key}.png') as image:
                    frames.append(normalize(image, config[source]['os_bar_crop'], config['size']))
            diff = ImageChops.difference(*frames)
            # Any RGB difference counts. No fuzzy color threshold or ignored controls.
            changed = sum(any(pixel) for pixel in diff.getdata())
            percent = 100 * changed / (diff.width * diff.height)
            diff.save(output/f'{key}-diff.png')
            for source, frame in zip(('ios', platform), frames):
                frame.save(output/f'{key}-{source}.png')
            results.append({'screen': key, 'different_pixels_percent': percent, 'passed': percent <= threshold})
        except (OSError, ValueError, KeyError) as error:
            results.append({'screen': key, 'different_pixels_percent': None, 'passed': False, 'error': str(error)})
    report = {'platform': platform, 'fixture': manifests[0], 'threshold_percent': threshold, 'normalization': config,
              'metric': 'percentage of pixels with any RGB channel difference', 'screens': results,
              'passed': all(row['passed'] for row in results)}
    (output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    lines = [f'# iOS / {platform} parity', '', f"Viewport: {config.get('viewport', 'unspecified')}. Threshold: {threshold:g}% (any RGB difference).", '', '| Screen | Different pixels | Result |', '|---|---:|---|']
    for row in results:
        percent = row['different_pixels_percent']
        value = f'{percent:.4f}%' if percent is not None else 'ERROR: '+row['error']
        lines.append(f"| {row['screen']} | {value} | {'PASS' if row['passed'] else 'FAIL'} |")
    (output/'report.md').write_text('\n'.join(lines)+'\n')
    print('\n'.join(lines))
    return report['passed']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('ios', 'output', 'config'):
        parser.add_argument('--'+flag, type=Path, required=True)
    candidates = parser.add_mutually_exclusive_group(required=True)
    candidates.add_argument('--android', type=Path)
    candidates.add_argument('--web', type=Path)
    parser.add_argument('--threshold', type=float, default=2.5)
    args = parser.parse_args()
    try:
        passed = compare(args.ios, args.web or args.android, args.output, json.loads(args.config.read_text()), args.threshold, 'web' if args.web else 'android')
    except (OSError, ValueError, KeyError) as error:
        parser.exit(2, f'Parity capture contract failed: {error}\n')
    raise SystemExit(0 if passed else 1)

if __name__ == '__main__':
    main()
