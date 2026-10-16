// Zoen's marble glass, one colour shader for every button and card.
//
// Wabi's production `.glass-btn` (design-reference-rea-20261014/wabi-static/wabi-page-css.css)
// gives the four flat layers: the -75deg fill, the inset shading, the conic edge that turns
// on press and the travelling shine. On top sits the volume that makes it read as a marble:
// absorption at the thick shoulder, the top lip's inner shadow, a specular arc, a caustic
// pool at the bottom and a Fresnel rim. The real refraction of what is behind comes from
// Liquid Glass (.glassEffect) under this layer; see ZoenGlass.swift.
#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;

static float sq(float x) { return x * x; }

static float roundedDistance(float2 p, float2 size, float r) {
    float2 h = size * 0.5;
    float2 q = abs(p - h) - (h - r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;   // < 0 inside
}

/// Position 0…1 along a CSS linear-gradient angle (0° points up, clockwise).
static float alongCSS(float2 p, float2 size, float degreesCSS) {
    float a = degreesCSS * M_PI_F / 180.0;
    float2 dir = float2(sin(a), -cos(a));
    float halfLength = 0.5 * (abs(size.x * dir.x) + abs(size.y * dir.y));
    return dot(p - size * 0.5, dir) / (2.0 * halfLength) + 0.5;
}

[[ stitchable ]] half4 zoenGlass(float2 position, half4 color, float2 size, float radius,
                                 float rimAngle, float shine, float press, float prominent, float strength) {
    if (color.a <= 0.0h) return half4(0.0h);
    float2 c = size * 0.5;
    float r = min(radius, min(c.x, c.y));
    float depth = -roundedDistance(position, size, r);           // px in from the edge
    float coverage = clamp(depth + 0.5, 0.0, 1.0);
    float y = position.y / size.y;
    float2 e = float2(0.75, 0.0);
    float2 outward = float2(roundedDistance(position + e.xy, size, r) - roundedDistance(position - e.xy, size, r),
                            roundedDistance(position + e.yx, size, r) - roundedDistance(position - e.yx, size, r));
    outward = outward / max(length(outward), 0.0001);
    float top = smoothstep(0.2, -0.85, outward.y);                // faces the light
    float bottom = smoothstep(0.1, 0.9, outward.y);
    float bevel = max(min(r, 28.0) * 0.9, 6.0);                   // the curved shoulder of the marble
    float edge = 1.0 - clamp(depth / bevel, 0.0, 1.0);

    // Wabi 1: the -75deg translucent fill
    float t = alongCSS(position, size, -75.0);
    float light = mix(0.05, 0.20, 1.0 - abs(t * 2.0 - 1.0)) * mix(1.0, 0.55, prominent);
    // Wabi 2: inset shading
    float dark = (1.0 - smoothstep(0.0, 0.22, y)) * 0.05;
    light += smoothstep(0.70, 1.0, y) * 0.35 * smoothstep(0.0, 3.0, depth);

    // Volume: glass is thicker at the shoulder, so it absorbs more there (deeper at the bottom)
    dark += edge * edge * 0.20 * mix(0.55, 1.0, y) * strength;
    // inner shadow cast by the top lip
    dark += exp(-depth / (bevel * 0.35)) * top * 0.16 * strength;
    // Specular: a bright arc tucked under the top lip, hottest top-left; slides inward on press
    float lip = bevel * mix(0.30, 0.42, press);
    float arc = exp(-sq((depth - lip) / (bevel * 0.16))) * top;
    float hot = exp(-sq((position.x - size.x * 0.30) / max(size.x * 0.28, bevel)));
    light += arc * (0.42 + 0.45 * hot) * strength;
    // Caustic: light focused through the body pools as a glow inside the bottom shoulder
    float pool = exp(-sq((depth - bevel * 0.45) / (bevel * 0.30))) * bottom;
    light += pool * (0.22 + 0.18 * press) * strength;
    // Fresnel rim: every edge catches light, the top one most
    light += sq(sq(edge)) * mix(0.18, 0.45, top);

    // Wabi 3: the conic edge, turning on press
    float2 d = position - c;
    float turn = fract((atan2(d.y, d.x) * 57.29578 + 90.0 - rimAngle) / 360.0 + 1.0);
    float conic = max(max(1.0 - smoothstep(0.0, 0.05, turn), smoothstep(0.95, 1.0, turn)),
                      smoothstep(0.40, 0.50, turn) * (1.0 - smoothstep(0.50, 0.60, turn)));
    float ring = 1.0 - smoothstep(0.0, 1.25, depth);
    dark += ring * conic * 0.35;
    // Wabi 4: the travelling shine band
    float u = (alongCSS(position, size, -45.0) + shine) * 0.5;
    light += smoothstep(0.20, 0.40, u) * (1.0 - smoothstep(0.50, 0.55, u)) * mix(0.35, 0.12, prominent);

    light = clamp(light, 0.0, 1.0);
    dark = clamp(dark, 0.0, 1.0);
    float alpha = light + dark - light * dark;
    float white = light * (1.0 - dark);                           // premultiplied
    return half4(half3(white), half(alpha)) * color.a * half(coverage);
}
