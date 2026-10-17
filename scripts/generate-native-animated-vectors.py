#!/usr/bin/env python3
"""Android native vector counterparts of the iOS navigation SF Symbol silhouettes.

Keep geometry in 26pt coordinates and motion on the vector group, not a bitmap
or Compose container. Durations mirror NativeIconMotion's discrete effects.
Run this generator after changing a contour; generated resources are committed.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / 'android/app/src/main/res/drawable'
GLYPHS = {
    'chats': 'M3,4 Q1.5,4 1.5,6 L1.5,14 Q1.5,16 3.5,16 L5,16 L5,20 L10,16 L15,16 Q17,16 17,14 L17,6 Q17,4 15,4 Z M19.5,9 L21.5,9 Q24,9 24,11 L24,19 Q24,21 22,21 L21,21 L21,24 L17,21 L12,21 Q10,21 10,19',
    'spaces': 'M4,3 L10,3 Q11,3 11,4 L11,10 Q11,11 10,11 L4,11 Q3,11 3,10 L3,4 Q3,3 4,3 Z M16,3 L22,3 Q23,3 23,4 L23,10 Q23,11 22,11 L16,11 Q15,11 15,10 L15,4 Q15,3 16,3 Z M4,15 L10,15 Q11,15 11,16 L11,22 Q11,23 10,23 L4,23 Q3,23 3,22 L3,16 Q3,15 4,15 Z M16,15 L22,15 Q23,15 23,16 L23,22 Q23,23 22,23 L16,23 Q15,23 15,22 L15,16 Q15,15 16,15 Z',
    'bell': 'M4,19 Q7,16 7,10 C7,1 19,1 19,10 Q19,16 22,19 Z M10,22 Q13,26 16,22 M13,2 L13,3',
    'marketplace': 'M3,3 L23,3 L25,9 Q25,13 21,13 Q18,13 18,10 Q18,13 13,13 Q8,13 8,10 Q8,13 5,13 Q1,13 1,9 Z M4,14 L4,25 L22,25 L22,14 M10,25 L10,17 L16,17 L16,25',
    'compose': 'M15,4 L5,4 Q3,4 3,6 L3,21 Q3,23 5,23 L20,23 Q22,23 22,21 L22,12 M11,16 L12,12 L22,2 L25,5 L15,15 Z',
    'plus': 'M13,4 L13,22 M4,13 L22,13',
    'mic': 'M9,6 C9,0 17,0 17,6 L17,13 C17,19 9,19 9,13 Z M5,12 L5,13 C5,24 21,24 21,13 L21,12 M13,21 L13,25 M9,25 L17,25',
    'send': 'M13,23 L13,3 M5,11 L13,3 L21,11',
    'back': 'M16,4 L7,13 L16,22',
    'close': 'M6,6 L20,20 M20,6 L6,20',
    'check': 'M4,13 L10,20 L23,5',
    'more': 'M5,13 L5.01,13 M13,13 L13.01,13 M21,13 L21.01,13',
    'search': 'M19,11 A8,8 0,1 1,3,11 A8,8 0,1 1,19,11 M17,17 L24,24',
    'phone': 'M4,2 L9,2 L11,8 L8,11 Q11,17 16,19 L19,16 L25,18 L25,23 Q25,26 21,25 C10,23 2,15 1,5 Q1,2 4,2 Z',
    'video': 'M3,6 L16,6 Q18,6 18,8 L18,19 Q18,21 16,21 L3,21 Q1,21 1,19 L1,8 Q1,6 3,6 Z M18,10 L25,6 L25,21 L18,17',
    'camera': 'M3,7 L8,7 L10,3 L17,3 L19,7 L23,7 Q25,7 25,9 L25,22 L1,22 L1,9 Q1,7 3,7 Z M19,14 A6,6 0,1 1,7,14 A6,6 0,1 1,19,14',
    'info': 'M25,13 A12,12 0,1 1,1,13 A12,12 0,1 1,25,13 M13,11 L13,20 M13,6 L13.01,6',
    'pin': 'M8,3 L20,3 L18,6 L18,12 L22,16 L6,16 L10,12 L10,6 Z M14,16 L14,25',
    'settings': 'M11,1 L15,1 L16,5 L19,6 L23,5 L25,9 L22,12 L22,15 L25,18 L23,22 L19,21 L16,23 L15,26 L11,26 L10,23 L7,21 L3,22 L1,18 L4,15 L4,12 L1,9 L3,5 L7,6 L10,5 Z M18,13 A5,5 0,1 1,8,13 A5,5 0,1 1,18,13',
    'share': 'M8,10 L3,10 L3,24 L23,24 L23,10 L18,10 M13,17 L13,1 M7,7 L13,1 L19,7',
    'folder': 'M1,6 L10,6 L13,9 L24,9 L24,23 L1,23 Z',
    'store': 'M3,8 L23,8 L24,24 L2,24 Z M8,9 L8,6 C8,0 18,0 18,6 L18,9',
    'you': 'M25,13 A12,12 0,1 1,1,13 A12,12 0,1 1,25,13 M17,9 A4,4 0,1 1,9,9 A4,4 0,1 1,17,9 M4,21 Q13,11 22,21',
    'chevron': 'M10,4 L19,13 L10,22',
    'heart': 'M13,24 C-9,9 4,-6 13,5 C22,-6 35,9 13,24 Z',
    'note': 'M4,1 L16,1 L23,8 L23,25 L4,25 Z M16,1 L16,8 L23,8 M8,13 L19,13 M8,17 L19,17 M8,21 L15,21',
    'download': 'M25,13 A12,12 0,1 1,1,13 A12,12 0,1 1,25,13 M13,5 L13,20 M7,14 L13,20 L19,14',
    'lock': 'M5,12 L22,12 L22,25 L5,25 Z M8,12 L8,7 C8,-1 19,-1 19,7 L19,12',
    'trash': 'M3,6 L23,6 M8,6 L8,2 L18,2 L18,6 M5,6 L6,25 L20,25 L21,6 M10,10 L10,21 M16,10 L16,21',
    'play': 'M6,2 L24,13 L6,24 Z',
    'pause': 'M7,3 L7,23 M19,3 L19,23',
    'list': 'M3,5 L3.01,5 M8,5 L24,5 M3,13 L3.01,13 M8,13 L24,13 M3,21 L3.01,21 M8,21 L24,21',
    'sparkle': 'M13,1 Q13,12 24,13 Q13,13 13,25 Q13,13 1,13 Q13,13 13,1 Z M23,1 L23,7 M20,4 L26,4',
    'crown': 'M3,21 L1,5 L8,11 L13,2 L18,11 L25,5 L23,21 Z M4,25 L22,25',
    'sun': 'M19,13 A6,6 0,1 1,7,13 A6,6 0,1 1,19,13 M13,0 L13,4 M13,22 L13,26 M0,13 L4,13 M22,13 L26,13 M4,4 L6,6 M20,20 L22,22 M4,22 L6,20 M20,6 L22,4',
    'game': 'M7,7 L19,7 Q23,7 24,13 L26,20 Q26,25 22,23 L17,19 L9,19 L4,23 Q0,25 0,20 L2,13 Q3,7 7,7 Z M5,12 L5,18 M2,15 L8,15 M20,12 L20.01,12 M23,15 L23.01,15',
    'agents': 'M13,9 A4,4 0,1 1,5,9 A4,4 0,1 1,13,9 M1,23 Q1,15 9,15 Q17,15 17,23 M17,5 Q24,8 17,12 M21,3 Q29,9 21,15',
}
# Settings silhouettes follow the SF names in SettingsScreens.swift.
GLYPHS.update({
    'language': 'M2,4 L15,4 L15,17 L10,17 L6,21 L6,17 L2,17 Z M8,6 L8,14 M4,9 L12,9 M5,9 Q5,13 12,15 M11,9 Q10,13 4,15 M16,14 L20,4 L25,18 M18,13 L23,13',
    'contrast': 'M25,13 A12,12 0,1 1,1,13 A12,12 0,1 1,25,13 M13,1 L13,25 M16,3 L16,23 M19,5 L19,21 M22,8 L22,18',
    'devices': 'M1,3 L20,3 L20,17 L1,17 Z M10,17 L10,22 M5,23 L15,23 M18,9 L25,9 L25,25 L18,25 Z',
    'storage': 'M3,5 L23,5 L23,11 L3,11 Z M3,15 L23,15 L23,21 L3,21 Z M6,8 L6.01,8 M6,18 L6.01,18',
    'cloud': 'M6,21 C-2,21 -1,9 6,9 C5,-1 21,-1 21,10 C29,10 28,21 21,21 Z',
    'accessibility': 'M16,4 A3,3 0,1 1,10,4 A3,3 0,1 1,16,4 M2,9 L24,9 M13,9 L13,17 M13,17 L7,25 M13,17 L19,25',
    'clock': 'M25,13 A12,12 0,1 1,1,13 A12,12 0,1 1,25,13 M13,5 L13,13 L19,16',
    'compass': 'M25,13 A12,12 0,1 1,1,13 A12,12 0,1 1,25,13 M8,18 L11,11 L18,8 L15,15 Z',
    'flag': 'M4,25 L4,2 M4,3 Q9,-1 15,3 Q20,7 24,3 L24,16 Q20,20 15,16 Q9,12 4,16',
    'gift': 'M2,10 L24,10 L24,15 L2,15 Z M4,15 L4,25 L22,25 L22,15 M13,10 L13,25 M13,10 C-6,8 5,-10 13,10 C32,-10 35,9 13,10',
    'wallet': 'M2,5 L24,5 L24,23 L2,23 Z M2,10 L24,10 M6,18 L12,18',
    'key': 'M12,8 A5,5 0,1 1,2,8 A5,5 0,1 1,12,8 M11,11 L24,24 M17,17 L21,13 M21,21 L25,17',
    'at': 'M18,8 L18,17 Q24,19 24,12 C24,-3 0,-2 1,14 Q1,27 20,23 M18,13 C18,4 7,6 7,13 C7,22 18,21 18,13',
})
# UITabBar uses the filled variants for these three system images.
GLYPHS.update({
    'chats_tab': 'M4,3 L17,3 Q19,3 19,5 L19,7 L12,7 Q8,7 8,11 L8,20 L4,17 L3,17 Q1,17 1,15 L1,6 Q1,3 4,3 Z M13,9 L23,9 Q25,9 25,11 L25,18 Q25,20 23,20 L22,20 L22,24 L17,20 L13,20 Q10,20 10,18 L10,12 Q10,9 13,9 Z',
    'spaces_tab': GLYPHS['spaces'],
    'bell_tab': 'M4,19 Q7,16 7,10 C7,1 19,1 19,10 Q19,16 22,19 Z M10,22 Q13,26 16,22 Z',
})
HEADER = '<?xml version="1.0" encoding="utf-8"?>\n'
for name, path in GLYPHS.items():
    fill = '#FF000000' if name.endswith('_tab') else '#00000000'
    vector = f'''<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="26dp" android:height="26dp" android:viewportWidth="26" android:viewportHeight="26">
    <group android:name="symbol" android:pivotX="13" android:pivotY="13">
        <path android:name="contour" android:pathData="{path}" android:fillColor="{fill}" android:strokeColor="#FF000000" android:strokeWidth="1.8" android:strokeLineCap="round" android:strokeLineJoin="round"/>
    </group>
</vector>\n'''
    (ROOT / f'native_{name}.xml').write_text(HEADER + vector)
    for effect, property_name, values, duration in [
        ('bounce', 'translateY', [0, -4, 0], 134),
        ('wiggle', 'rotation', [0, -12, 12, -8, 8, 0], 70),
        ('active', 'strokeAlpha', [1, .45, 1], 400),
        ('draw', 'trimPathEnd', [0, 1], 260),
    ]:
        if name.endswith('_tab') and effect in ('active', 'draw'):
            property_name = 'fillAlpha'
        target = 'contour' if effect in ('active', 'draw') else 'symbol'
        animators = '\n'.join(f'                <objectAnimator android:propertyName="{property_name}" android:valueFrom="{a}" android:valueTo="{b}" android:valueType="floatType" android:duration="{duration}" android:interpolator="@android:interpolator/fast_out_slow_in"/>' for a,b in zip(values, values[1:]))
        (ROOT / f'native_{name}_{effect}.xml').write_text(HEADER + f'''<animated-vector xmlns:android="http://schemas.android.com/apk/res/android" xmlns:aapt="http://schemas.android.com/aapt" android:drawable="@drawable/native_{name}">
    <target android:name="{target}">
        <aapt:attr name="android:animation">
            <set android:ordering="sequentially">
{animators}
            </set>
        </aapt:attr>
    </target>
</animated-vector>\n''')
values = ROOT.parent / 'values/native_symbols.xml'
values.write_text(HEADER + '<resources><string-array name="native_symbol_names">' + ''.join(f'<item>{name}</item>' for name in GLYPHS) + '</string-array></resources>\n')
print(f'Generated {len(GLYPHS)} native vector glyphs with four effects each')
