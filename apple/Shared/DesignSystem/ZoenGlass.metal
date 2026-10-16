// Original rounded-lens optics shared conceptually with Android's AGSL material.
#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;
static float roundedDistance(float2 p, float2 size, float r) {
    float2 q = abs(p - size * .5) - (size * .5 - r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
[[ stitchable ]] half4 zoenGlass(float2 position, SwiftUI::Layer layer, float2 size, float radius,
                                float press, float prominent, float strength, float frost, float slab) {
    float r = max(1.0, min(radius, min(size.x, size.y) * .5));
    float depth = -roundedDistance(position, size, r);
    float coverage = clamp(depth + .5, 0.0, 1.0);
    float2 e = float2(.5, 0);
    float2 outward = float2(roundedDistance(position+e.xy,size,r)-roundedDistance(position-e.xy,size,r),
                             roundedDistance(position+e.yx,size,r)-roundedDistance(position-e.yx,size,r));
    outward /= max(length(outward), .0001);
    // A full-radius crown, not a thin bevel. The small-control preset has less sag.
    float edge = 1.0 - clamp(depth / r, 0.0, 1.0);
    float slope = edge * mix(.98, .55, slab);
    float3 n = float3(outward * slope, sqrt(max(1.0-slope*slope, .0001)));
    float3 ray = refract(float3(0,0,-1), n, 1.0/1.5);
    float2 bend = ray.xy * r * edge * edge * mix(.65,.18,slab) * (1.0-.15*press);
    float2 samplePoint = clamp(position + bend, float2(.5), size-float2(.5));
    float2 dispersion = bend * .04 * (1.0-slab);
    half4 base = layer.sample(samplePoint);
    float3 rgb = float3(layer.sample(clamp(samplePoint+dispersion,float2(.5),size-.5)).r,
                        base.g, layer.sample(clamp(samplePoint-dispersion,float2(.5),size-.5)).b);
    float fresnel = .035 + .965 * pow(1.0-n.z,5.0);
    float3 light = normalize(float3(-.55,-.65,.65));
    float3 halfway = normalize(light + float3(0,0,1));
    float spec = pow(max(dot(n,normalize(halfway+float3(0,.08*press,0))),0.0),72.0);
    float broad = pow(max(dot(n,halfway),0.0),9.0);
    float innerShadow = edge*edge * (.12 + .32*max(dot(outward,float2(.6,.8)),0.0));
    float caustic = exp(-pow((edge-.66)/.18,2.0)) * max(dot(outward,normalize(float2(.55,.8))),0.0);
    float rim = fresnel * .65 + exp(-depth*depth/1.8)*.24;
    float3 fringe = float3(.04,0,.04) * edge*edge*(1.0-slab)
        * float3(max(-outward.x,0.0),0,max(outward.x,0.0));
    rgb = rgb * (1.0-innerShadow*strength) + (spec*.9 + broad*.065 + rim*.48 + caustic*.16)*strength + fringe;
    float alpha = float(base.a);
    return half4(half3(clamp(rgb,0.0,alpha)),half(alpha)) * half(coverage);
}
