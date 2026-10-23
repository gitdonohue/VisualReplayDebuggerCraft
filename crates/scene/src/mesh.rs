//! Triangle tessellation of the primitives a capture can draw.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use vrd_model::{Color, Transform};

/// A lit triangle vertex. Colours are linear RGBA.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

/// One line segment, drawn as a screen-space-width quad. Colours are linear RGBA.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct LineSegment {
    pub a: [f32; 3],
    /// Width in pixels.
    pub width: f32,
    pub b: [f32; 3],
    pub _pad: f32,
    pub color: [f32; 4],
}

impl LineSegment {
    pub fn new(a: Vec3, b: Vec3, color: [f32; 4], width: f32) -> Self {
        Self { a: a.to_array(), width, b: b.to_array(), _pad: 0.0, color }
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// A capture colour as linear RGBA with the given alpha.
pub fn linear_color(color: Color, alpha: f32) -> [f32; 4] {
    let [r, g, b, a] = color.rgba();
    [
        srgb_to_linear(f32::from(r) / 255.0),
        srgb_to_linear(f32::from(g) / 255.0),
        srgb_to_linear(f32::from(b) / 255.0),
        alpha * f32::from(a) / 255.0,
    ]
}

/// Linear RGBA from an sRGB colour given as 0-255 bytes.
pub fn linear_rgba(r: u8, g: u8, b: u8, a: f32) -> [f32; 4] {
    [
        srgb_to_linear(f32::from(r) / 255.0),
        srgb_to_linear(f32::from(g) / 255.0),
        srgb_to_linear(f32::from(b) / 255.0),
        a,
    ]
}

/// The rigid matrix for a capture transform. A degenerate rotation counts as identity.
pub fn transform_matrix(t: &Transform) -> Mat4 {
    let q = Quat::from_xyzw(t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w);
    let q = if q.is_finite() && q.length_squared() > 1e-12 { q.normalize() } else { Quat::IDENTITY };
    Mat4::from_rotation_translation(q, Vec3::new(t.translation.x, t.translation.y, t.translation.z))
}

pub fn to_vec3(p: vrd_model::Point) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

/// Applies a rigid transform to vertices in place.
pub fn transform_vertices(vertices: &mut [Vertex], m: Mat4) {
    for v in vertices {
        v.position = m.transform_point3(Vec3::from(v.position)).to_array();
        v.normal = m.transform_vector3(Vec3::from(v.normal)).normalize_or_zero().to_array();
    }
}

fn quad(out: &mut Vec<Vertex>, p: [Vec3; 4], n: [Vec3; 4], color: [f32; 4]) {
    for i in [0, 1, 2, 0, 2, 3] {
        out.push(Vertex { position: p[i].to_array(), normal: n[i].to_array(), color });
    }
}

/// Revolves a profile of `(radius, z, normal_radius, normal_z)` points about the Z axis.
fn revolve(out: &mut Vec<Vertex>, profile: &[(f32, f32, f32, f32)], segments: usize, color: [f32; 4]) {
    let ring = |angle: f32, p: (f32, f32, f32, f32)| {
        let (s, c) = angle.sin_cos();
        (Vec3::new(p.0 * c, p.0 * s, p.1), Vec3::new(p.2 * c, p.2 * s, p.3))
    };
    for pair in profile.windows(2) {
        for i in 0..segments {
            let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
            let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
            let (p00, n00) = ring(a0, pair[0]);
            let (p01, n01) = ring(a1, pair[0]);
            let (p10, n10) = ring(a0, pair[1]);
            let (p11, n11) = ring(a1, pair[1]);
            // Counter-clockwise seen from outside, for profiles that run from the +Z pole downwards.
            quad(out, [p00, p10, p11, p01], [n00, n10, n11, n01], color);
        }
    }
}

/// A UV sphere with its poles on the Z axis.
pub fn sphere(out: &mut Vec<Vertex>, center: Vec3, radius: f32, color: [f32; 4]) {
    const RINGS: usize = 12;
    let profile: Vec<_> = (0..=RINGS)
        .map(|i| {
            let theta = std::f32::consts::PI * i as f32 / RINGS as f32;
            let (s, c) = theta.sin_cos();
            (radius * s, radius * c, s, c)
        })
        .collect();
    let start = out.len();
    revolve(out, &profile, 24, color);
    transform_vertices(&mut out[start..], Mat4::from_translation(center));
}

/// A capsule: the points within `radius` of the segment `p1`-`p2`.
pub fn capsule(out: &mut Vec<Vertex>, p1: Vec3, p2: Vec3, radius: f32, color: [f32; 4]) {
    const HEMI: usize = 6;
    let axis = p2 - p1;
    let len = axis.length();
    // Top pole to bottom pole, like the sphere profile, so the winding comes out the same way.
    let mut profile = Vec::with_capacity(2 * (HEMI + 1));
    for i in 0..=HEMI {
        let a = std::f32::consts::FRAC_PI_2 * (1.0 - i as f32 / HEMI as f32);
        let (s, c) = a.sin_cos();
        profile.push((radius * c, len + radius * s, c, s));
    }
    for i in 0..=HEMI {
        let a = -std::f32::consts::FRAC_PI_2 * i as f32 / HEMI as f32;
        let (s, c) = a.sin_cos();
        profile.push((radius * c, radius * s, c, s));
    }
    let start = out.len();
    revolve(out, &profile, 16, color);
    let rotation = if len > 1e-6 { Quat::from_rotation_arc(Vec3::Z, axis / len) } else { Quat::IDENTITY };
    transform_vertices(&mut out[start..], Mat4::from_rotation_translation(rotation, p1));
}

/// A box centred on the origin of `transform`, with `dimensions` as its full extent along each local axis.
pub fn cuboid(out: &mut Vec<Vertex>, transform: &Transform, dimensions: Vec3, color: [f32; 4]) {
    let h = dimensions.abs() * 0.5;
    let corner = |sx: f32, sy: f32, sz: f32| Vec3::new(sx * h.x, sy * h.y, sz * h.z);
    let faces: [([Vec3; 4], Vec3); 6] = [
        ([corner(1., -1., -1.), corner(1., 1., -1.), corner(1., 1., 1.), corner(1., -1., 1.)], Vec3::X),
        ([corner(-1., 1., -1.), corner(-1., -1., -1.), corner(-1., -1., 1.), corner(-1., 1., 1.)], Vec3::NEG_X),
        ([corner(1., 1., -1.), corner(-1., 1., -1.), corner(-1., 1., 1.), corner(1., 1., 1.)], Vec3::Y),
        ([corner(-1., -1., -1.), corner(1., -1., -1.), corner(1., -1., 1.), corner(-1., -1., 1.)], Vec3::NEG_Y),
        ([corner(-1., -1., 1.), corner(1., -1., 1.), corner(1., 1., 1.), corner(-1., 1., 1.)], Vec3::Z),
        ([corner(-1., 1., -1.), corner(1., 1., -1.), corner(1., -1., -1.), corner(-1., -1., -1.)], Vec3::NEG_Z),
    ];
    let start = out.len();
    for (p, n) in faces {
        quad(out, p, [n; 4], color);
    }
    transform_vertices(&mut out[start..], transform_matrix(transform));
}

/// A triangle soup with flat normals. Trailing vertices that do not fill a triangle are ignored.
pub fn triangle_soup(out: &mut Vec<Vertex>, points: &[Vec3], color: [f32; 4]) {
    for tri in points.as_chunks::<3>().0 {
        let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        for p in tri {
            out.push(Vertex { position: p.to_array(), normal: n.to_array(), color });
        }
    }
}

/// Segments of a circle in the plane through `center` perpendicular to `normal`.
pub fn circle_segments(
    center: Vec3,
    normal: Vec3,
    radius: f32,
    color: [f32; 4],
    width: f32,
    out: &mut Vec<LineSegment>,
) {
    const DIVISIONS: usize = 40;
    let Some(n) = normal.try_normalize() else {
        return;
    };
    let helper = if n.dot(Vec3::Z).abs() < 0.99 { Vec3::Z } else { Vec3::X };
    let u = n.cross(helper).normalize();
    let v = n.cross(u);
    let at = |i: usize| {
        let a = std::f32::consts::TAU * i as f32 / DIVISIONS as f32;
        center + (u * a.cos() + v * a.sin()) * radius
    };
    for i in 0..DIVISIONS {
        out.push(LineSegment::new(at(i), at(i + 1), color, width));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrd_model::{Point, Quat as FQuat};

    fn bounds(v: &[Vertex]) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for v in v {
            lo = lo.min(Vec3::from(v.position));
            hi = hi.max(Vec3::from(v.position));
        }
        (lo, hi)
    }

    const WHITE: [f32; 4] = [1.0; 4];

    /// Every non-degenerate triangle must wind counter-clockwise around its own normal, because the
    /// shader decides which side of a surface it is looking at from the winding.
    fn assert_winding_matches_normals(v: &[Vertex], what: &str, at_least: usize) {
        let mut checked = 0;
        for tri in v.as_chunks::<3>().0 {
            let (a, b, c) = (Vec3::from(tri[0].position), Vec3::from(tri[1].position), Vec3::from(tri[2].position));
            let cross = (b - a).cross(c - a);
            if cross.length() < 1e-4 {
                continue;
            }
            let n = (Vec3::from(tri[0].normal) + Vec3::from(tri[1].normal) + Vec3::from(tri[2].normal)).normalize();
            assert!(cross.normalize().dot(n) > 0.5, "{what}: triangle {tri:?} is wound against its normal");
            checked += 1;
        }
        assert!(checked >= at_least, "{what}: checked only {checked} triangles");
    }

    #[test]
    fn winding_agrees_with_normals_for_every_primitive() {
        let mut v = Vec::new();
        sphere(&mut v, Vec3::new(1.0, 2.0, 3.0), 5.0, WHITE);
        assert_winding_matches_normals(&v, "sphere", 200);

        let mut v = Vec::new();
        capsule(&mut v, Vec3::ZERO, Vec3::new(0.0, 0.0, 100.0), 10.0, WHITE);
        assert_winding_matches_normals(&v, "capsule along +Z", 100);

        let mut v = Vec::new();
        capsule(&mut v, Vec3::new(5.0, 5.0, 5.0), Vec3::new(-40.0, 12.0, 70.0), 10.0, WHITE);
        assert_winding_matches_normals(&v, "tilted capsule", 100);

        let mut v = Vec::new();
        capsule(&mut v, Vec3::new(0.0, 0.0, 100.0), Vec3::ZERO, 10.0, WHITE);
        assert_winding_matches_normals(&v, "capsule along -Z", 100);

        let mut v = Vec::new();
        let t =
            Transform { translation: Point::new(3.0, 4.0, 5.0), rotation: FQuat { x: 0.5, y: 0.5, z: 0.5, w: 0.5 } };
        cuboid(&mut v, &t, Vec3::new(4.0, 5.0, 6.0), WHITE);
        assert_winding_matches_normals(&v, "rotated box", 12);
    }

    #[test]
    fn sphere_is_centred_and_round() {
        let mut v = Vec::new();
        sphere(&mut v, Vec3::new(10.0, 20.0, 30.0), 5.0, WHITE);
        assert_eq!(v.len() % 3, 0);
        for vert in &v {
            let p = Vec3::from(vert.position);
            assert!(((p - Vec3::new(10.0, 20.0, 30.0)).length() - 5.0).abs() < 1e-3);
            let n = Vec3::from(vert.normal);
            assert!((n.length() - 1.0).abs() < 1e-3);
            assert!(n.dot((p - Vec3::new(10.0, 20.0, 30.0)).normalize()) > 0.99, "normals point outwards");
        }
        let (lo, hi) = bounds(&v);
        assert!((hi.z - lo.z - 10.0).abs() < 1e-2);
    }

    #[test]
    fn capsule_spans_both_endpoints() {
        let mut v = Vec::new();
        capsule(&mut v, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 100.0), 10.0, WHITE);
        let (lo, hi) = bounds(&v);
        assert!((lo.z + 10.0).abs() < 1e-3 && (hi.z - 110.0).abs() < 1e-3, "{lo:?} {hi:?}");
        assert!((hi.x - 10.0).abs() < 1e-3);

        // Along an arbitrary axis the extent is the segment plus the radius at each end.
        let mut w = Vec::new();
        let (a, b) = (Vec3::new(1.0, 2.0, 3.0), Vec3::new(41.0, 2.0, 3.0));
        capsule(&mut w, a, b, 5.0, WHITE);
        let (lo, hi) = bounds(&w);
        assert!((lo.x - -4.0).abs() < 1e-3 && (hi.x - 46.0).abs() < 1e-3, "{lo:?} {hi:?}");
    }

    #[test]
    fn zero_length_capsule_is_a_sphere_not_nan() {
        let mut v = Vec::new();
        capsule(&mut v, Vec3::ONE, Vec3::ONE, 3.0, WHITE);
        assert!(v.iter().all(|v| v.position.iter().all(|c| c.is_finite())));
    }

    #[test]
    fn box_has_the_right_extent_and_outward_normals() {
        let mut v = Vec::new();
        let t = Transform { translation: Point::new(100.0, 0.0, 0.0), rotation: FQuat::IDENTITY };
        cuboid(&mut v, &t, Vec3::new(10.0, 20.0, 30.0), WHITE);
        assert_eq!(v.len(), 36);
        let (lo, hi) = bounds(&v);
        assert_eq!(lo, Vec3::new(95.0, -10.0, -15.0));
        assert_eq!(hi, Vec3::new(105.0, 10.0, 15.0));
        for tri in v.as_chunks::<3>().0 {
            let face_normal = (Vec3::from(tri[1].position) - Vec3::from(tri[0].position))
                .cross(Vec3::from(tri[2].position) - Vec3::from(tri[0].position))
                .normalize();
            assert!(face_normal.dot(Vec3::from(tri[0].normal)) > 0.99, "winding agrees with the normal");
        }
    }

    #[test]
    fn rotated_box_rotates_about_its_centre() {
        let mut v = Vec::new();
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let t = Transform { translation: Point::ZERO, rotation: FQuat { x: 0.0, y: 0.0, z: half, w: half } };
        cuboid(&mut v, &t, Vec3::new(10.0, 2.0, 2.0), WHITE);
        let (lo, hi) = bounds(&v);
        assert!((hi.y - 5.0).abs() < 1e-4 && (hi.x - 1.0).abs() < 1e-4, "{lo:?} {hi:?}");
    }

    #[test]
    fn soup_computes_flat_normals_and_drops_leftovers() {
        let mut v = Vec::new();
        let pts = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z];
        triangle_soup(&mut v, &pts, WHITE);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].normal, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn circle_stays_on_its_plane_at_its_radius() {
        let mut segs = Vec::new();
        let center = Vec3::new(5.0, 5.0, 5.0);
        circle_segments(center, Vec3::new(1.0, 1.0, 0.0), 7.0, WHITE, 1.0, &mut segs);
        assert_eq!(segs.len(), 40);
        let n = Vec3::new(1.0, 1.0, 0.0).normalize();
        for s in &segs {
            let a = Vec3::from(s.a) - center;
            assert!(a.dot(n).abs() < 1e-3);
            assert!((a.length() - 7.0).abs() < 1e-3);
        }
        let mut none = Vec::new();
        circle_segments(center, Vec3::ZERO, 7.0, WHITE, 1.0, &mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn colours_convert_to_linear() {
        let red = linear_color(Color::RED, 1.0);
        assert_eq!(red, [1.0, 0.0, 0.0, 1.0]);
        let gray = linear_color(Color::from_name("Gray").unwrap(), 0.5);
        assert!((gray[0] - 0.2158).abs() < 1e-3 && (gray[3] - 0.5).abs() < 1e-6);
        assert_eq!(linear_color(Color(44), 1.0)[3], 0.0);
    }

    #[test]
    fn degenerate_rotation_is_identity() {
        let t =
            Transform { translation: Point::new(1.0, 2.0, 3.0), rotation: FQuat { x: 0.0, y: 0.0, z: 0.0, w: 0.0 } };
        assert_eq!(transform_matrix(&t), Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)));
    }
}
