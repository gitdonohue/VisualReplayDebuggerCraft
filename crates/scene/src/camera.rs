use glam::{Mat4, Vec2, Vec3, Vec4};

const MAX_PITCH: f32 = 1.5;
const MIN_DISTANCE: f32 = 1.0e-3;
const MAX_DISTANCE: f32 = 1.0e9;

/// A world axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Axis {
    X,
    Y,
    #[default]
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn vector(self) -> Vec3 {
        match self {
            Axis::X => Vec3::X,
            Axis::Y => Vec3::Y,
            Axis::Z => Vec3::Z,
        }
    }

    /// The two horizontal axes when this axis is up, ordered so that `h1 x h2 = up`.
    pub fn horizontal(self) -> (Vec3, Vec3) {
        match self {
            Axis::X => (Vec3::Y, Vec3::Z),
            Axis::Y => (Vec3::Z, Vec3::X),
            Axis::Z => (Vec3::X, Vec3::Y),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }
}

/// The local direction cameras face in engines that put `up` up: the original tool's Z-up
/// convention faces -X, and Y-up engines conventionally face down -Z.
pub fn default_camera_forward(up: Axis) -> Vec3 {
    match up {
        Axis::Z => Vec3::NEG_X,
        Axis::X | Axis::Y => Vec3::NEG_Z,
    }
}

/// An orbit camera: it looks at `target` from `distance` away.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    /// Which world axis is up.
    pub up_axis: Axis,
    /// Rotation about the up axis, in radians.
    pub yaw: f32,
    /// Elevation above the horizontal plane, in radians (positive looks down at the target).
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { target: Vec3::ZERO, up_axis: Axis::Z, yaw: 0.6, pitch: 0.5, distance: 20.0, fov_y: 45f32.to_radians() }
    }
}

impl Camera {
    pub fn up_vector(&self) -> Vec3 {
        self.up_axis.vector()
    }

    /// Unit vector from the target to the eye.
    fn offset_dir(&self) -> Vec3 {
        let (h1, h2) = self.up_axis.horizontal();
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        h1 * (cp * cy) + h2 * (cp * sy) + self.up_vector() * sp
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.offset_dir() * self.distance
    }

    pub fn forward(&self) -> Vec3 {
        -self.offset_dir()
    }

    /// The up reference for building a view basis, avoiding the pole where it is parallel to `forward`.
    fn reference_up(&self, forward: Vec3) -> Vec3 {
        if forward.dot(self.up_vector()).abs() > 0.9999 { self.up_axis.horizontal().1 } else { self.up_vector() }
    }

    pub fn right(&self) -> Vec3 {
        let f = self.forward();
        f.cross(self.reference_up(f)).normalize()
    }

    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward())
    }

    /// Distance to the near clip plane. The far plane is at infinity (the projection is reversed-Z).
    pub fn near(&self) -> f32 {
        (self.distance * 0.005).max(1.0e-4)
    }

    pub fn view(&self) -> Mat4 {
        let f = self.forward();
        look_at_rh(self.eye(), self.target, self.reference_up(f))
    }

    /// Reversed-Z, infinite-far perspective projection (depth 1 at the near plane, 0 at infinity).
    pub fn projection(&self, aspect: f32) -> Mat4 {
        perspective_infinite_reverse_rh(self.fov_y, aspect.max(1e-3), self.near())
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// Orbits around the target. `dx`/`dy` are mouse movement in pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.006;
        self.pitch = (self.pitch + dy * 0.006).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Slides the camera parallel to the view plane. `dx`/`dy` are mouse movement in pixels.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height_px: f32) {
        let per_pixel = 2.0 * self.distance * (self.fov_y * 0.5).tan() / viewport_height_px.max(1.0);
        self.target += (-dx * self.right() + dy * self.up()) * per_pixel;
    }

    /// Moves towards or away from the target. Positive `scroll` zooms in.
    pub fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (-scroll * 0.0025).exp()).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Looks at a sphere so that all of it is in view.
    pub fn focus(&mut self, center: Vec3, radius: f32) {
        self.target = center;
        let radius = radius.max(1.0e-3);
        self.distance = (radius / (self.fov_y * 0.5).sin() * 1.15).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Places the eye at `position` looking along `look`, keeping the current pivot distance.
    pub fn set_pose(&mut self, position: Vec3, look: Vec3, pivot_distance: f32) {
        let Some(look) = look.try_normalize() else {
            return;
        };
        self.distance = pivot_distance.clamp(MIN_DISTANCE, MAX_DISTANCE);
        self.target = position + look * self.distance;
        let back = -look;
        let (h1, h2) = self.up_axis.horizontal();
        self.yaw = back.dot(h2).atan2(back.dot(h1));
        self.pitch = back.dot(self.up_vector()).clamp(-1.0, 1.0).asin().clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Projects a world position to viewport pixels (origin top-left). `None` if behind the camera.
    pub fn project(&self, world: Vec3, viewport_px: Vec2) -> Option<Projected> {
        let clip = self.view_proj(viewport_px.x / viewport_px.y.max(1.0)) * world.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(Projected {
            screen: Vec2::new((ndc.x * 0.5 + 0.5) * viewport_px.x, (0.5 - ndc.y * 0.5) * viewport_px.y),
            distance: clip.w,
        })
    }

    /// Pixels covered by one world unit at `distance` in front of the camera.
    pub fn pixels_per_unit(&self, distance: f32, viewport_height_px: f32) -> f32 {
        viewport_height_px / (2.0 * distance.max(1e-4) * (self.fov_y * 0.5).tan())
    }
}

/// Right-handed view matrix looking from `eye` towards `target`.
fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        Vec4::new(s.x, u.x, -f.x, 0.0),
        Vec4::new(s.y, u.y, -f.y, 0.0),
        Vec4::new(s.z, u.z, -f.z, 0.0),
        Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

/// Right-handed projection with depth 1 at `near` falling to 0 at infinity (wgpu clip space).
fn perspective_infinite_reverse_rh(fov_y: f32, aspect: f32, near: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols(
        Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        Vec4::new(0.0, f, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, -1.0),
        Vec4::new(0.0, 0.0, near, 0.0),
    )
}

/// A world position in viewport space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projected {
    pub screen: Vec2,
    /// Distance in front of the camera along its view axis.
    pub distance: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Vec2 = Vec2::new(800.0, 600.0);

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 0.5
    }

    fn cameras() -> Vec<Camera> {
        Axis::ALL.iter().map(|&up_axis| Camera { up_axis, ..Camera::default() }).collect()
    }

    #[test]
    fn target_projects_to_the_centre() {
        for cam in cameras() {
            let cam = Camera { target: Vec3::new(500.0, -200.0, 50.0), ..cam };
            let p = cam.project(cam.target, VIEWPORT).unwrap();
            assert!(close(p.screen, VIEWPORT / 2.0), "{:?}: {p:?}", cam.up_axis);
            assert!((p.distance - cam.distance).abs() < 0.01);
        }
    }

    #[test]
    fn points_behind_the_camera_do_not_project() {
        let cam = Camera::default();
        let behind = cam.eye() - cam.forward() * 10.0;
        assert!(cam.project(behind, VIEWPORT).is_none());
    }

    #[test]
    fn the_up_axis_is_up_on_screen_for_every_convention() {
        for cam in cameras() {
            let above = cam.project(cam.target + cam.up_vector() * 1.0, VIEWPORT).unwrap();
            let centre = cam.project(cam.target, VIEWPORT).unwrap();
            assert!(above.screen.y < centre.screen.y, "{:?}: world up must be screen up", cam.up_axis);
            assert!((above.screen.x - centre.screen.x).abs() < 0.01, "and not lean sideways at zero roll");
        }
    }

    #[test]
    fn right_is_right_on_screen() {
        for cam in cameras() {
            let p = cam.project(cam.target + cam.right() * 1.0, VIEWPORT).unwrap();
            assert!(p.screen.x > VIEWPORT.x / 2.0);
        }
    }

    #[test]
    fn horizontal_axes_complete_a_right_handed_basis() {
        for axis in Axis::ALL {
            let (h1, h2) = axis.horizontal();
            assert_eq!(h1.cross(h2), axis.vector(), "{axis:?}");
        }
    }

    #[test]
    fn orbit_keeps_distance_and_clamps_pitch() {
        for mut cam in cameras() {
            let d = (cam.eye() - cam.target).length();
            cam.orbit(120.0, -40.0);
            assert!(((cam.eye() - cam.target).length() - d).abs() < 0.01);
            cam.orbit(0.0, 1.0e6);
            assert!(cam.pitch <= MAX_PITCH);
            assert!(cam.view().is_finite());
            cam.orbit(0.0, -1.0e7);
            assert!(cam.pitch >= -MAX_PITCH);
        }
    }

    #[test]
    fn zoom_scales_distance_within_limits() {
        let mut cam = Camera::default();
        let d = cam.distance;
        cam.zoom(100.0);
        assert!(cam.distance < d);
        cam.zoom(-100.0);
        assert!((cam.distance - d).abs() < 0.01);
        cam.zoom(1.0e6);
        assert!(cam.distance >= MIN_DISTANCE);
        cam.zoom(-1.0e6);
        assert!(cam.distance <= MAX_DISTANCE);
    }

    #[test]
    fn pan_moves_the_target_with_the_content() {
        for mut cam in cameras() {
            let anchor = cam.target;
            let before = cam.project(anchor, VIEWPORT).unwrap().screen;
            cam.pan(50.0, 0.0, VIEWPORT.y);
            let after = cam.project(anchor, VIEWPORT).unwrap().screen;
            // Dragging right by 50px drags the scene right by 50px.
            assert!(close(after, before + Vec2::new(50.0, 0.0)), "{before:?} -> {after:?}");
        }
    }

    #[test]
    fn focus_frames_the_sphere() {
        let mut cam = Camera::default();
        let centre = Vec3::new(1000.0, 2000.0, 300.0);
        cam.focus(centre, 400.0);
        let edge = centre + cam.right() * 400.0;
        let p = cam.project(edge, VIEWPORT).unwrap().screen;
        assert!(p.x > 0.0 && p.x < VIEWPORT.x && p.y > 0.0 && p.y < VIEWPORT.y);
    }

    #[test]
    fn set_pose_puts_the_eye_at_the_position_looking_along_the_direction() {
        for mut cam in cameras() {
            let pos = Vec3::new(10.0, 20.0, 30.0);
            let look = Vec3::new(-1.0, 0.5, -0.2).normalize();
            cam.set_pose(pos, look, 100.0);
            assert!((cam.eye() - pos).length() < 0.01, "{:?}: {:?}", cam.up_axis, cam.eye());
            assert!(cam.forward().dot(look) > 0.9999, "{:?}", cam.up_axis);
        }
    }

    #[test]
    fn straight_down_is_not_degenerate() {
        for mut cam in cameras() {
            let down = -cam.up_vector();
            cam.set_pose(Vec3::new(0.0, 0.0, 500.0), down, 100.0);
            assert!(cam.view().is_finite());
            assert!(cam.right().is_finite() && cam.up().is_finite());
        }
    }

    #[test]
    fn depth_is_reversed() {
        let cam = Camera::default();
        let vp = cam.view_proj(1.0);
        let depth = |p: Vec3| {
            let c = vp * p.extend(1.0);
            c.z / c.w
        };
        let near = depth(cam.eye() + cam.forward() * cam.near() * 2.0);
        let far = depth(cam.eye() + cam.forward() * cam.distance * 10.0);
        assert!(near > far && far > 0.0);
    }

    #[test]
    fn conventional_camera_forward_axes() {
        assert_eq!(default_camera_forward(Axis::Z), Vec3::NEG_X);
        assert_eq!(default_camera_forward(Axis::Y), Vec3::NEG_Z);
    }
}
