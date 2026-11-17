// Bloom chain: bright-pass -> downsample x N -> additive upsample x N -> composite.

struct Params {
    threshold: f32,
    knee: f32,
    intensity: f32,
    exposure: f32,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> p: Params;
@group(0) @binding(3) var bloom_tex: texture_2d<f32>; // composite only

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VOut;
    o.pos = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return o;
}

fn s(uv: vec2<f32>) -> vec3<f32> {
    return textureSample(src, samp, uv).rgb;
}

// Dual-filter (Kawase) downsample: 5 bilinear taps.
fn down(uv: vec2<f32>) -> vec3<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    var c = s(uv) * 4.0;
    c += s(uv + vec2<f32>(-t.x, -t.y));
    c += s(uv + vec2<f32>(t.x, -t.y));
    c += s(uv + vec2<f32>(-t.x, t.y));
    c += s(uv + vec2<f32>(t.x, t.y));
    return c / 8.0;
}

@fragment
fn fs_bright(in: VOut) -> @location(0) vec4<f32> {
    let c = down(in.uv);
    let br = max(c.r, max(c.g, c.b));
    var rq = clamp(br - p.threshold + p.knee, 0.0, 2.0 * p.knee);
    rq = rq * rq / (4.0 * p.knee + 1e-5);
    let w = max(rq, br - p.threshold) / max(br, 1e-5);
    return vec4<f32>(c * w, 1.0);
}

@fragment
fn fs_down(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(down(in.uv), 1.0);
}

// 3x3 tent upsample; result is additively blended onto the larger level.
@fragment
fn fs_up(in: VOut) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    let uv = in.uv;
    var c = s(uv) * 4.0;
    c += (s(uv + vec2<f32>(-t.x, 0.0)) + s(uv + vec2<f32>(t.x, 0.0)) + s(uv + vec2<f32>(0.0, -t.y)) + s(uv + vec2<f32>(0.0, t.y))) * 2.0;
    c += s(uv + vec2<f32>(-t.x, -t.y)) + s(uv + vec2<f32>(t.x, -t.y)) + s(uv + vec2<f32>(-t.x, t.y)) + s(uv + vec2<f32>(t.x, t.y));
    return vec4<f32>(c / 16.0, 1.0);
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_composite(in: VOut) -> @location(0) vec4<f32> {
    let scene = textureSample(src, samp, in.uv).rgb;
    let bloom = textureSample(bloom_tex, samp, in.uv).rgb;
    let c = (scene + bloom * p.intensity) * p.exposure;
    return vec4<f32>(aces(c), 1.0);
}
