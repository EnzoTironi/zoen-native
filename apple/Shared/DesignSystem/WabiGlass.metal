// Wabi's glass button, as a SwiftUI colour shader.
//
// Ported from wabi.ai's production `.glass-btn` (sotabot/website-v1,
// src/components/ui/glassmorphic-button, captured 2026-10-14 in
// design-reference-rea-20261014/wabi-static/wabi-page-css.css). The CSS draws it in
// five layers; this draws the same five in one pass over a capsule:
//   1. the -75° translucent fill            linear-gradient(-75deg, .05, .2, .05)
//   2. inset shading: dark under the top, lit above the bottom, a soft white rim glow
//   3. the conic edge: a 1px ring, dark at two opposite points, clear between
//   4. the travelling shine band             (-45°, white .5 at 40–50%, screen)
// The backdrop blur (layer 0) is the SwiftUI material under this view.
#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;

static float capsuleDistance(float2 p, float2 size) {
    float2 c = size * 0.5;
    float r = min(c.x, c.y);
    float2 q = abs(p - c) - (c - r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;   // < 0 inside
}

/// Position 0…1 along a CSS linear-gradient angle (0° points up, clockwise).
static float alongCSS(float2 p, float2 size, float degreesCSS) {
    float a = degreesCSS * M_PI_F / 180.0;
    float2 dir = float2(sin(a), -cos(a));
    float halfLength = 0.5 * (abs(size.x * dir.x) + abs(size.y * dir.y));
    return dot(p - size * 0.5, dir) / (2.0 * halfLength) + 0.5;
}

[[ stitchable ]] half4 wabiGlass(float2 position, half4 color, float2 size,
                                 float rimAngle, float shine, float prominent) {
    if (color.a <= 0.0h) return half4(0.0h);
    float depth = -capsuleDistance(position, size);          // px in from the edge
    float y = position.y / size.y;

    // 1. the diagonal fill
    float t = alongCSS(position, size, -75.0);
    float light = mix(0.05, 0.20, 1.0 - abs(t * 2.0 - 1.0)) * mix(1.0, 0.55, prominent);

    // 2. inset 0 .125em .125em rgba(0,0,0,.035) / inset 0 -.125em .125em rgba(255,255,255,.5)
    float dark = (1.0 - smoothstep(0.0, 0.22, y)) * 0.07;
    light += smoothstep(0.70, 1.0, y) * 0.5 * smoothstep(0.0, 3.0, depth);
    light += (1.0 - smoothstep(0.0, 4.0, depth)) * 0.2;       // 0 0 .1em .25em inset white .2

    // 3. the conic edge, from var(--angle-1): dark at 0% and 50%, clear 5–40% and 60–95%
    float2 d = position - size * 0.5;
    float turn = fract((atan2(d.y, d.x) * 180.0 / M_PI_F + 90.0 - rimAngle) / 360.0 + 1.0);
    float conic = max(max(1.0 - smoothstep(0.0, 0.05, turn), smoothstep(0.95, 1.0, turn)),
                      smoothstep(0.40, 0.50, turn) * (1.0 - smoothstep(0.50, 0.60, turn)));
    float ring = 1.0 - smoothstep(0.0, 1.25, depth);
    dark += ring * conic * 0.5;
    light += ring * mix(0.5, 0.0, y);                          // the white half under the conic

    // 4. the shine: a 200%-wide gradient whose position slides 0% → 25% on press
    float u = (alongCSS(position, size, -45.0) + shine) * 0.5;
    light += smoothstep(0.20, 0.40, u) * (1.0 - smoothstep(0.50, 0.55, u)) * mix(0.5, 0.16, prominent);

    light = saturate(light);
    dark = saturate(dark);
    float alpha = light + dark - light * dark;
    float white = light * (1.0 - dark);                        // premultiplied
    return half4(half3(white), half(alpha)) * color.a;
}
