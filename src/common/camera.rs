use glam::EulerRot;

use crate::{
    Quaternion, Vector3,
    common::transform::{to_gl_pos, to_gl_rot},
};

#[derive(Clone, Copy, Debug)]
pub struct Camera<T> {
    pub position: [T; 3],
    pub right: [T; 3],
    pub up: [T; 3],
    pub forward: [T; 3],
}

impl Camera<f32> {
    pub fn new() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            forward: [0.0, 0.0, 1.0],
        }
    }

    /// Returns the orthogonal matrix (view matrix)
    pub fn orthogonal(&self) -> [[f32; 4]; 4] {
        let [px, py, pz] = self.position;
        let [rx, ry, rz] = self.right;
        let [ux, uy, uz] = self.up;
        let [fx, fy, fz] = self.forward;

        [
            [rx, ux, fx, 0.0],
            [ry, uy, fy, 0.0],
            [rz, uz, fz, 0.0],
            [
                -(rx * px + ry * py + rz * pz),
                -(ux * px + uy * py + uz * pz),
                -(fx * px + fy * py + fz * pz),
                1.0,
            ],
        ]
    }
}

impl Default for Camera<f32> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CameraPerspective<T> {
    pub fov: T,
    pub near_clip: T,
    pub far_clip: T,
    pub aspect_ratio: T,
}

impl CameraPerspective<f32> {
    /// Returns the perspective projection matrix
    pub fn projection(&self) -> [[f32; 4]; 4] {
        let f = 1.0 / (self.fov.to_radians() / 2.0).tan();
        let nf = 1.0 / (self.near_clip - self.far_clip);

        [
            [f / self.aspect_ratio, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, (self.far_clip + self.near_clip) * nf, -1.0],
            [0.0, 0.0, 2.0 * self.far_clip * self.near_clip * nf, 0.0],
        ]
    }
}

pub fn create_camera() -> Camera<f32> {
    Camera::new()
}

pub fn set_camera(camera: &mut Camera<f32>, pos: Vector3, angle: Vector3) {
    camera.position = to_gl_pos(pos).into();

    let rotation = Quaternion::from_euler(
        EulerRot::XYZ,
        angle.x.to_radians(),
        angle.y.to_radians(),
        angle.z.to_radians(),
    );
    let rotation = to_gl_rot(rotation);
    camera.right = (rotation * Vector3::X).into();
    camera.up = (rotation * Vector3::Y).into();
    camera.forward = (rotation * Vector3::Z).into();
}
