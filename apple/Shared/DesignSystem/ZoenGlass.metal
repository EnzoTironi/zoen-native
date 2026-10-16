// Zoen's marble glass, one colour shader for every button and card.
//
// Wabi's production `.glass-btn` (design-reference-rea-20261014/wabi-static/wabi-page-css.css)
// gives the conic edge that turns on press and the travelling shine. The volume follows the
// frame analysis of Wabi's lens (ui-refs/wabi-glass/VIDEO-ANALYSIS.md): a hemisphere normal
// across the shoulder, light from normalize(-.7,.7,1), a specular that is sharp when clear and
// broad when frosted, a 1.5% white rim brighter top-left, a bottom-right inner shadow and a
// white film from .05 (clear) to .25 (frosted). The real refraction of what is behind comes from
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
                                 float rimAngle, float shine, float press, float prominent, float strength, float frost) {
    if (color.a <= 0.0h) return half4(0.0h);
    float2 c = size * 0.5;
    float r = min(radius, min(c.x, c.y));
    float depth = -roundedDistance(position, size, r);           // px in from the edge
    float coverage = clamp(depth + 0.5, 0.0, 1.0);
    float2 e = float2(0.75, 0.0);
    float2 outward = float2(roundedDistance(position + e.xy, size, r) - roundedDistance(position - e.xy, size, r),
                            roundedDistance(position + e.yx, size, r) - roundedDistance(position - e.yx, size, r));
    outward = outward / max(length(outward), 0.0001);
    // Hemisphere profile across the shoulder (Wabi's lens: the outer band bends hardest).
    float bevel = max(min(r, 28.0), 4.0);
    float s = 1.0 - clamp(depth / bevel, 0.0, 1.0);               // 1 at the edge, 0 on the crown
    float3 n = normalize(float3(outward * s, sqrt(max(1.0 - s * s, 0.0))));
    float3 L = normalize(float3(-0.7, -0.7, 1.0));                // top-left (y grows down)
    float3 H = normalize(L + float3(0.0, 0.0, 1.0));
    float clear = 1.0 - frost;                                    // Wabi: clear lens ↔ frosted

    // Tint film: white .05 clear → .25 frosted
    float light = mix(0.05, 0.25, frost) * mix(1.0, 0.4, prominent);
    float dark = 0.0;
    // Specular: sharp (roughness .1) when clear, broad (.7) when frosted; slides on press
    float rough = mix(0.1, 0.7, frost);
    float3 Hp = normalize(H + float3(0.12, 0.12, 0.0) * press);
    float spec = pow(max(dot(n, Hp), 0.0), mix(400.0, 12.0, rough));
    light += spec * mix(0.9, 0.45, frost) * strength;
    // Specular streak: the light source mirrored along the top shoulder (reads as a marble, not a dot)
    float facing = smoothstep(0.2, -0.85, outward.y);
    float streak = exp(-sq((depth - bevel * mix(0.32, 0.45, press)) / (bevel * 0.16))) * facing;
    light += streak * mix(0.55, 0.30, frost) * (0.6 + 0.4 * smoothstep(size.x, 0.0, position.x)) * strength;
    // Lambert falloff gives the body its volume: shoulders facing away from the light darken
    float lambert = dot(n, L);
    dark += (1.0 - clamp(lambert * 1.25, 0.0, 1.0)) * 0.22 * s * strength;
    // Inner shadow, bottom-right, 15-20% of the radius deep, 10-20% black
    float br = clamp(dot(outward, normalize(float2(1.0, 1.0))), 0.0, 1.0);
    dark += exp(-depth / max(bevel * 0.18, 1.0)) * br * 0.18 * strength;
    // Rim: thin white, 1.5% of the radius (≥ 1px), brighter top-left
    float rimW = max(r * 0.015 * 2.0, 1.0);
    float rim = 1.0 - smoothstep(0.0, rimW, depth);
    float tl = clamp(dot(outward, normalize(float2(-1.0, -1.0))) * 0.5 + 0.5, 0.0, 1.0);
    light += rim * mix(0.25, 0.85, tl);
    // Wabi's conic edge turns on press
    float2 d = position - c;
    float turn = fract((atan2(d.y, d.x) * 57.29578 + 90.0 - rimAngle) / 360.0 + 1.0);
    float conic = max(max(1.0 - smoothstep(0.0, 0.05, turn), smoothstep(0.95, 1.0, turn)),
                      smoothstep(0.40, 0.50, turn) * (1.0 - smoothstep(0.50, 0.60, turn)));
    dark += rim * conic * 0.25 * clear;
    // Wabi's travelling shine band
    float u = (alongCSS(position, size, -45.0) + shine) * 0.5;
    light += smoothstep(0.20, 0.40, u) * (1.0 - smoothstep(0.50, 0.55, u)) * mix(0.25, 0.10, prominent);

    light = clamp(light, 0.0, 1.0);
    dark = clamp(dark, 0.0, 1.0);
    float alpha = light + dark - light * dark;
    float white = light * (1.0 - dark);                           // premultiplied
    return half4(half3(white), half(alpha)) * color.a * half(coverage);
}
