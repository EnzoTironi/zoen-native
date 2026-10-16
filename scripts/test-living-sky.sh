#!/bin/bash
# Exercises the production Swift model without the optional native core binary.
set -euo pipefail
cd "$(dirname "$0")/.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
python3 - "$work/main.swift" <<'PY'
import sys
source = open('apple/Shared/DesignSystem/LivingSky.swift').read()
model = source[source.index('enum SkyClock'):source.index('/// A quiet')]
checks = '''
for h in SkyClock.hours {
    let a = SkyClock.sample(h - 0.00001), b = SkyClock.sample(h + 0.00001)
    for i in 0..<3 { precondition(abs(a.rgb[i] - b.rgb[i]) < 1e-8) }
    precondition(abs(a.stars - b.stars) < 1e-8)
}
precondition(SkyClock.sample(0).stars == 1)
precondition(SkyClock.sample(12).stars == 0)
precondition(SkyClock.sample(-1).rgb == SkyClock.sample(23).rgb)
precondition(SkyClock.sample(.nan).rgb == SkyClock.sample(12).rgb)
for minute in 0..<1440 {
    let sky = SkyClock.sample(Double(minute) / 60)
    precondition(sky.rgb.allSatisfy { $0 >= 0 && $0 <= 1 })
    precondition(sky.stars >= 0 && sky.stars <= 1)
}
print("PASS: Swift sky continuity, midnight wrap, invalid inputs, 1440 samples")
'''
open(sys.argv[1], 'w').write('import Foundation\n' + model + checks)
PY
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer swift "$work/main.swift"
node --test packages/zoen-ui/web/living-sky.test.ts
