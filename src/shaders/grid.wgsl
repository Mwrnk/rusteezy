// Draws the whole character grid in one instanced call.
// Instance = cell. The 6 vertices of each instance form the cell quad.

struct Uniforms {
    screen: vec2<f32>,     // render target size in px
    cell: vec2<f32>,       // cell size in px (== atlas glyph size)
    origin: vec2<f32>,     // top-left of the grid in px
    cols: u32,             // grid columns
    atlas_cols: u32,       // glyph slots per atlas row
    glow_gain: f32,        // how much `glow` brightens the glyph
    _p0: f32,
    _p1: f32,
    _p2: f32,
};

struct Cell {
    fg: vec4<f32>,         // rgb + glow
    bg: vec3<f32>,
    glyph: u32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> cells: array<Cell>;
@group(0) @binding(2) var atlas: texture_2d<f32>;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) id: u32,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[vi];
    let col = f32(ii % u.cols);
    let row = f32(ii / u.cols);
    let px = u.origin + (vec2<f32>(col, row) + c) * u.cell;
    var out: VOut;
    out.pos = vec4<f32>(px.x / u.screen.x * 2.0 - 1.0, 1.0 - px.y / u.screen.y * 2.0, 0.0, 1.0);
    out.local = c * u.cell;
    out.id = ii;
    return out;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let cell = cells[in.id];
    let cs = vec2<u32>(u.cell);
    let base = vec2<u32>(cell.glyph % u.atlas_cols, cell.glyph / u.atlas_cols) * cs;
    let lp = min(vec2<u32>(max(in.local, vec2<f32>(0.0))), cs - vec2<u32>(1u));
    let a = textureLoad(atlas, base + lp, 0).r;
    let fg = cell.fg.rgb * (1.0 + cell.fg.w * u.glow_gain);
    return vec4<f32>(mix(cell.bg, fg, a), 1.0);
}
