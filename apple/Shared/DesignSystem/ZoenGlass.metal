// Zoen's marble glass, one colour shader for every button and card.
//
// Our own implementation of the lens measured in Wabi's app (ui-refs/wabi-glass/app/NOTES.md):
// a hemisphere normal across the rail, Schlick Fresnel 0.035 + 0.965(1-cos)^5, a key light
// 27deg / 65deg high with highlights 1.41, frost as the only whitening and a red/blue
// dispersion fringe on the rail. `slab` 1 is Wabi's button preset (thin rail, no dispersion,
// clearcoat .4); 0 is the full marble for hero buttons and cards. The refraction of what is
// behind comes from Liquid Glass (.glassEffect) under this layer; see ZoenGlass.swift.
#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;

static float roundedDistance(float2 p, float2 size, float r) {
    float2 h = size * 0.5;
    float2 q = abs(p - h) - (h - r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;   // < 0 inside
}

[[ stitchable ]] half4 zoenGlass(float2 position, half4 color, float2 size, float radius,
                                 float press, float prominent, float strength, float frost, float slab) {
    if (color.a <= 0.0h) return half4(0.0h);
    const float unit = 1.0;                                      // SwiftUI positions are already points
    float2 c = size * 0.5;
    float r = min(radius, min(c.x, c.y));
    float depth = -roundedDistance(position, size, r);           // px in from the edge
    float coverage = clamp(depth + 0.5, 0.0, 1.0);
    float2 e = float2(0.75 * unit, 0.0);
    float2 outward = float2(roundedDistance(position + e.xy, size, r) - roundedDistance(position - e.xy, size, r),
                            roundedDistance(position + e.yx, size, r) - roundedDistance(position - e.yx, size, r));
    outward = outward / max(length(outward), 0.0001);
    // The lens profile: a hemisphere across the rail. A marble's rail is the whole radius;
    // the slab preset's is a thin bevel, so small buttons read as a slab with a rounded lip.
    float rail = max(mix(min(r, 28.0 * unit), min(r, 28.0 * unit) * 0.35, slab), 3.0 * unit);
    float edge = 1.0 - clamp(depth / rail, 0.0, 1.0);             // 0 on the crown, 1 at the rim
    float3 n = normalize(float3(outward * edge, sqrt(max(1.0 - edge * edge, 0.0))));
    float3 V = float3(0.0, 0.0, 1.0);
    // Key light 27deg around from straight up-left, 65deg high
    float az = -2.0420352;                                       // -90deg - 27deg
    float el = 1.1344640;                                        // 65deg
    float3 L = normalize(float3(cos(az) * cos(el), sin(az) * cos(el), sin(el)));
    float3 H = normalize(L + V);

    // Schlick Fresnel: almost clear facing you, mirror-bright at grazing (the rim)
    float cosT = clamp(n.z, 0.0, 1.0);
    float fresnel = 0.035 + 0.965 * pow(1.0 - cosT, 5.0);
    // Specular, roughness .116 (marble) / .14 (slab), highlights 1.41 / clearcoat .4; press slides it
    float rough = mix(0.116, 0.14, slab);
    float3 Hp = normalize(H + float3(0.0, 0.10, 0.0) * press);
    float spec = pow(max(dot(n, Hp), 0.0), 0.5 / (rough * rough)) * mix(1.41, 0.4, slab) * 0.7
               * smoothstep(mix(0.08, 0.62, prominent), mix(0.45, 0.9, prominent), edge);   // only the curved rail glints: the crown stays clear behind the label
    // Frost lifts the body toward white (the only whitening: no separate tint film)
    float frostWhite = frost * mix(0.16, 0.03, prominent);       // a coloured body stays deep enough for white text
    // Dispersion fringe: red out, blue in, only on the rail (strength = dispersion x edge^2)
    float disp = mix(0.81, 0.0, slab) * edge * edge;
    float3 fringe = float3(1.0, 0.0, -1.0) * disp * 0.10;
    // Lambert shading of the rail gives the body its volume; the far side falls into shadow
    float shade = (1.0 - clamp(dot(n, L) * 1.2, 0.0, 1.0)) * edge * 0.30 * strength;

    float3 rgb = float3(fresnel * 0.9 * strength + spec * strength + frostWhite) + fringe;
    float lum = clamp(max(max(rgb.r, rgb.g), rgb.b), 0.0, 1.0);
    float alpha = clamp(lum + shade * (1.0 - lum), 0.0, 1.0);
    rgb = clamp(rgb, 0.0, 1.0) * (1.0 - shade);                   // premultiplied
    return half4(half3(rgb), half(alpha)) * color.a * half(coverage);
}
