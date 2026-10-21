#!/usr/bin/env python3
"""Reproduce the bounded, namespaced Telegram video-control port at one pin."""
from pathlib import Path
import hashlib
import json
import re
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PIN = "f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838"
BASE = f"https://raw.githubusercontent.com/TelegramMessenger/Telegram-iOS/{PIN}/"
EXPECTED = {
    "TGMediaPickerGalleryVideoScrubber.h": "dc7c944b4d1eacfa0cf034280716cd342812f67780d8163519c5bbb4c988b629",
    "TGMediaPickerGalleryVideoScrubber.m": "71462d1df6964a875363dcd31304c3bcc57d569b64749b0ab6bfa993a1d9eaa4",
    "TGMediaPickerGalleryVideoScrubberThumbnailView.h": "49260086b119bf336b6b3f045cd5e749c9794d0ab122f6039d05b091e952f325",
    "TGMediaPickerGalleryVideoScrubberThumbnailView.m": "e51f467d97af5d99ea339f66a69c5efc66ffa25fc0e6a97a5b53e170aaa486af",
    "TGMediaPickerGalleryVideoTrimView.h": "77fb7841d6311ffd031f8c2356331b3b67ddd6baf3d6ba6798b66ee6e097d075",
    "TGMediaPickerGalleryVideoTrimView.m": "d401080335f77e4220327295aaca5e6f97b52e18f2fff353b70945b4c5e39cd2",
    "TGMediaPickerScrubberHeaderView.h": "ab4d3da3aac13557e9b1862465e25bf89d84051af079140ad2619b17af19a918",
    "TGMediaPickerScrubberHeaderView.m": "53b9995353959ec83b2e56f28c3b7d0abade036982f3757ce733e87f4e1ca9d3",
}


def transform(name, text):
    # The control algorithms stay upstream. Only module primitives and the one
    # linear frame-animation backend are supplied by the scoped UIKit adapter.
    text = re.sub(r'^#import (?:"LegacyComponentsInternal.h"|<LegacyComponents/[^>]+>)\n', '', text, flags=re.M)
    text = re.sub(r'\bTGMediaPicker', 'ZoenTGMediaPicker', text)
    if name.endswith('.h'):
        text = text.replace('#import <UIKit/UIKit.h>', '#import "ZoenVideoEditingPrimitives.h"')
    else:
        text = '#import "ZoenVideoEditingPrimitives.h"\n' + text
    text = text.replace(': UIControl', ': ZoenVideoHitControl')
    text = text.replace('UIControl *_wrapperView', 'ZoenVideoHitControl *_wrapperView')
    text = text.replace('UIControl *_scrubberHandle', 'ZoenVideoHitControl *_scrubberHandle')
    text = text.replace('UIControl *_dotHandle', 'ZoenVideoHitControl *_dotHandle')
    text = text.replace('[[UIControl alloc]', '[[ZoenVideoHitControl alloc]')
    text = text.replace('UIButton *_leftSegmentView', 'ZoenVideoHitButton *_leftSegmentView')
    text = text.replace('UIButton *_rightSegmentView', 'ZoenVideoHitButton *_rightSegmentView')
    text = text.replace('[[UIButton alloc]', '[[ZoenVideoHitButton alloc]')
    text = text.replace('TGSystemFontOfSize(12.0f)', '[UIFont systemFontOfSize:12.0f]')
    text = text.replace('TGScreenScaling()', 'self.traitCollection.displayScale')
    text = text.replace('TGSeparatorHeight()', 'ZoenVideoSeparatorHeight(self)')
    text = text.replace('UIColorRGBA(', 'ZoenVideoColorRGBA(').replace('UIColorRGB(', 'ZoenVideoColorRGB(')
    text = text.replace('TGTintedImage(', 'ZoenVideoTintedImage(')
    text = text.replace('TGRotationForOrientation(', 'ZoenVideoRotationForOrientation(')
    text = text.replace('TGRetinaPixel', '(self.traitCollection.displayScale > 1.0 ? 0.5 : 0.0)')
    text = text.replace('CGFloor(', 'floor(').replace('CGCeil(', 'ceil(')
    if name == 'TGMediaPickerGalleryVideoScrubber.m':
        text = text.replace('    UILabel *_currentTimeLabel;', '    ZoenVideoLinearFrameDriver *_frameDriver;\n    UILabel *_currentTimeLabel;')
        start = text.index('    POPBasicAnimation *animation = ')
        end = text.index('\n}', start)
        text = text[:start] + '''    if (_frameDriver == nil)
        _frameDriver = [[ZoenVideoLinearFrameDriver alloc] initWithView:_scrubberHandle];
    [_frameDriver startFromFrame:fromFrame toFrame:toFrame duration:duration];''' + text[end:]
        text = text.replace('    [_scrubberHandle pop_removeAnimationForKey:@"progress"];', '    [_frameDriver stop];')
        pos = text.rfind('@end')
        text = text[:pos] + '''- (void)zoenSuspend
{
    [self cancelZoomIn];
    [NSObject cancelPreviousPerformRequestsWithTarget:self];
    [self removeHandleAnimation];
    [self ignoreThumbnails];
}

''' + text[pos:]
    if name == 'TGMediaPickerGalleryVideoScrubber.h':
        text = text.replace('- (void)resetToStart;', '- (void)resetToStart;\n\n/// Zoen lifecycle fence; never changes selection or writes media.\n- (void)zoenSuspend;')
        text = text.replace('@protocol ZoenTGMediaPickerGalleryVideoScrubberDelegate <NSObject>', 'NS_SWIFT_UI_ACTOR\n@protocol ZoenTGMediaPickerGalleryVideoScrubberDelegate <NSObject>')
        text = text.replace('@protocol ZoenTGMediaPickerGalleryVideoScrubberDataSource <NSObject>', 'NS_SWIFT_UI_ACTOR\n@protocol ZoenTGMediaPickerGalleryVideoScrubberDataSource <NSObject>')
    return '// Adapted from Telegram-iOS at ' + PIN + '; GPL-2.0-or-later. See NOTICE.md.\n' + text


def main():
    verify = '--verify' in sys.argv
    manifest = {'revision': PIN, 'sources': []}
    for name, expected in EXPECTED.items():
        path = 'submodules/LegacyComponents/Sources/' + name
        raw = urllib.request.urlopen(BASE + path, timeout=30).read()
        assert hashlib.sha256(raw).hexdigest() == expected, 'upstream hash mismatch: ' + path
        output = transform(name, raw.decode()).encode()
        target = ROOT / 'Sources' / ('Zoen' + name)
        if verify:
            assert target.read_bytes() == output, 'port differs from recorded transform: ' + str(target)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(output)
        manifest['sources'].append({'path': path, 'url': BASE + path, 'bytes': len(raw), 'sha256': expected,
                                    'output': str(target.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output).hexdigest()})
    if not verify:
        (ROOT / 'upstream-sources.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'sources': len(EXPECTED), 'verified': verify, 'revision': PIN}))


if __name__ == '__main__':
    main()
