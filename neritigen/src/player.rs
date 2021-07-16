use std::f32::consts::TAU;

use nalgebra as na;

#[derive(Debug)]
pub struct Player {
    pub yaw: f32,   // 0..1; 0 = +x, 0.25 = +y
    pub pitch: f32, // -0.25..0.25; -0.25 = -z, 0.25 = +z
    pub position: na::Point3<f32>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            position: na::Point3::origin(),
        }
    }

    pub fn turn(&mut self, direction: na::Vector2<f32>) {
        self.yaw = (self.yaw - direction.x).rem_euclid(1.0);
        self.pitch = (self.pitch - direction.y).max(-0.25).min(0.25);
    }

    pub fn rotation(&self) -> na::Rotation3<f32> {
        na::Rotation3::from_euler_angles(0.0, -TAU * self.pitch, TAU * self.yaw)
    }

    pub fn movement_basis(&self) -> na::Matrix3<f32> {
        let mut basis = self.rotation().into_inner();
        basis.set_column(2, &na::Vector3::z_axis());
        basis
    }

    pub fn go(&mut self, direction: na::Vector3<f32>) {
        self.position += self.movement_basis() * direction
    }

    // maps from playerspace to worldspace
    pub fn isometry(&self) -> na::geometry::Isometry<f32, na::Rotation3<f32>, 3> {
        na::geometry::Isometry::from_parts(self.position.into(), self.rotation())
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}
