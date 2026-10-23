struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: camera position, w: near plane distance.
    eye: vec4<f32>,
    // xy: viewport size in pixels.
    viewport: vec4<f32>,
    light0: vec4<f32>,
    light1: vec4<f32>,
};

struct Model {
    matrix: mat4x4<f32>,
    tint: vec4<f32>,
    // rgb: colour to blend towards, a: blend amount.
    mix_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> model: Model;

struct MeshIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_mesh(in: MeshIn) -> MeshOut {
    var out: MeshOut;
    let world = model.matrix * vec4<f32>(in.position, 1.0);
    out.clip = globals.view_proj * world;
    out.normal = (model.matrix * vec4<f32>(in.normal, 0.0)).xyz;
    let tinted = in.color * model.tint;
    out.color = vec4<f32>(mix(tinted.rgb, model.mix_color.rgb, model.mix_color.a), tinted.a);
    return out;
}

// The output texture is plain RGBA8 holding gamma-encoded colour, which is what egui expects of
// the images it shows; all lighting is done in linear space and encoded here at the end.
fn encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn shade(color: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let key = max(dot(normal, globals.light0.xyz), 0.0) * 0.75;
    let fill = max(dot(normal, globals.light1.xyz), 0.0) * 0.30;
    return color * min(0.28 + key + fill, 1.2);
}

fn lit(in: MeshOut, front: bool) -> vec3<f32> {
    // Surfaces are lit from both sides: a triangle seen from behind uses its flipped normal.
    var n = normalize(in.normal);
    if (!front) {
        n = -n;
    }
    return shade(in.color.rgb, n);
}

@fragment
fn fs_solid(in: MeshOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    return vec4<f32>(encode(lit(in, front)), 1.0);
}

@fragment
fn fs_translucent(in: MeshOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    return vec4<f32>(encode(lit(in, front)), in.color.a);
}

struct LineIn {
    // xyz: start, w: width in pixels.
    @location(0) a: vec4<f32>,
    @location(1) b: vec4<f32>,
    @location(2) color: vec4<f32>,
};

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    // Signed distance from the line's centre, in pixels, and the line's half width.
    @location(1) across: vec2<f32>,
};

@vertex
fn vs_line(@builtin(vertex_index) vi: u32, in: LineIn) -> LineOut {
    var out: LineOut;
    out.color = in.color;

    var ca = globals.view_proj * vec4<f32>(in.a.xyz, 1.0);
    var cb = globals.view_proj * vec4<f32>(in.b.xyz, 1.0);

    // Clip the segment to the near plane (clip.w is the distance in front of the camera) so that
    // endpoints behind the camera do not flip the quad.
    let near_w = globals.eye.w;
    if (ca.w < near_w && cb.w < near_w) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
        out.across = vec2<f32>(0.0, 0.0);
        return out;
    }
    if (ca.w < near_w) {
        ca = mix(ca, cb, (near_w - ca.w) / (cb.w - ca.w));
    }
    if (cb.w < near_w) {
        cb = mix(cb, ca, (near_w - cb.w) / (ca.w - cb.w));
    }

    let half_view = globals.viewport.xy * 0.5;
    let na = ca.xy / ca.w;
    let nb = cb.xy / cb.w;
    let delta = (nb - na) * half_view;
    var dir = vec2<f32>(1.0, 0.0);
    if (length(delta) > 1e-4) {
        dir = normalize(delta);
    }
    let normal = vec2<f32>(-dir.y, dir.x);

    // Two triangles: (a-,a+,b-) and (b-,a+,b+).
    var end = array<f32, 6>(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);
    var side = array<f32, 6>(-1.0, 1.0, -1.0, -1.0, 1.0, 1.0);
    let e = end[vi];
    let s = side[vi];

    let half_width = in.a.w * 0.5 + 1.0;
    let base_c = select(ca, cb, e > 0.5);
    let base_n = base_c.xy / base_c.w;
    let offset_px = normal * s * half_width + dir * (e * 2.0 - 1.0) * half_width;
    let ndc = base_n + offset_px / half_view;

    // Pull lines slightly towards the camera so they win depth ties with the surfaces they lie on.
    out.clip = vec4<f32>(ndc * base_c.w, base_c.z * 1.0002, base_c.w);
    out.across = vec2<f32>(s * half_width, in.a.w * 0.5);
    return out;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    let coverage = clamp(in.across.y + 0.5 - abs(in.across.x), 0.0, 1.0);
    return vec4<f32>(encode(in.color.rgb), in.color.a * coverage);
}
