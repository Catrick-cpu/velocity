//! Small, dependency-free 3D math used by the world renderer.
//!
//! Matrices are column-major `[f32; 16]`, matching what OpenGL expects, so they
//! can be uploaded with `uniform_matrix_4_f32_slice(.., false, ..)` directly.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub const ZERO: Self = v3(0.0, 0.0, 0.0);
    pub const UP: Self = v3(0.0, 1.0, 0.0);

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        v3(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn horizontal(self) -> f32 {
        (self.x * self.x + self.z * self.z).sqrt()
    }

    pub fn distance_xz(self, other: Self) -> f32 {
        (self - other).horizontal()
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len > 1e-6 {
            self / len
        } else {
            Self::ZERO
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }

    pub fn scale(self, s: f32) -> Self {
        self * s
    }

    /// Rotate around the world Y axis.
    pub fn rotate_y(self, radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        v3(self.x * c + self.z * s, self.y, -self.x * s + self.z * c)
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl Add for V3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        v3(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for V3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for V3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        v3(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for V3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        v3(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Div<f32> for V3 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        v3(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for V3 {
    type Output = Self;
    fn neg(self) -> Self {
        v3(-self.x, -self.y, -self.z)
    }
}

/// Identity matrix, column-major.
pub const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

pub fn perspective(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y_rad * 0.5).tan();
    let mut m = [0.0f32; 16];
    m[0] = f / aspect.max(0.0001);
    m[5] = f;
    m[10] = (far + near) / (near - far);
    m[11] = -1.0;
    m[14] = (2.0 * far * near) / (near - far);
    m
}

pub fn ortho(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 2.0 / (right - left);
    m[5] = 2.0 / (top - bottom);
    m[10] = -2.0 / (far - near);
    m[12] = -(right + left) / (right - left);
    m[13] = -(top + bottom) / (top - bottom);
    m[14] = -(far + near) / (far - near);
    m[15] = 1.0;
    m
}

pub fn look_at(eye: V3, target: V3, up: V3) -> [f32; 16] {
    let f = (target - eye).normalized();
    let s = f.cross(up).normalized();
    let u = s.cross(f);
    [
        s.x,
        u.x,
        -f.x,
        0.0,
        s.y,
        u.y,
        -f.y,
        0.0,
        s.z,
        u.z,
        -f.z,
        0.0,
        -s.dot(eye),
        -u.dot(eye),
        f.dot(eye),
        1.0,
    ]
}

/// `a * b` in column-major convention (apply `b` first, then `a`).
pub fn mul(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = sum;
        }
    }
    out
}

pub fn mul_vec4(m: [f32; 16], v: [f32; 4]) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    for row in 0..4 {
        out[row] = m[row] * v[0] + m[4 + row] * v[1] + m[8 + row] * v[2] + m[12 + row] * v[3];
    }
    out
}

pub fn inverse(m: [f32; 16]) -> [f32; 16] {
    let mut inv = [0.0f32; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];

    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-12 {
        return IDENTITY;
    }
    let inv_det = 1.0 / det;
    for value in &mut inv {
        *value *= inv_det;
    }
    inv
}

/// Six frustum planes extracted from a view-projection matrix.
/// Each plane is `(nx, ny, nz, d)` with the normal pointing inwards.
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    pub fn from_view_projection(m: [f32; 16]) -> Self {
        let row = |i: usize| [m[i], m[4 + i], m[8 + i], m[12 + i]];
        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);
        let mut planes = [[0.0f32; 4]; 6];
        let mut set = |idx: usize, p: [f32; 4]| {
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-6);
            planes[idx] = [p[0] / len, p[1] / len, p[2] / len, p[3] / len];
        };
        set(0, [r3[0] + r0[0], r3[1] + r0[1], r3[2] + r0[2], r3[3] + r0[3]]);
        set(1, [r3[0] - r0[0], r3[1] - r0[1], r3[2] - r0[2], r3[3] - r0[3]]);
        set(2, [r3[0] + r1[0], r3[1] + r1[1], r3[2] + r1[2], r3[3] + r1[3]]);
        set(3, [r3[0] - r1[0], r3[1] - r1[1], r3[2] - r1[2], r3[3] - r1[3]]);
        set(4, [r3[0] + r2[0], r3[1] + r2[1], r3[2] + r2[2], r3[3] + r2[3]]);
        set(5, [r3[0] - r2[0], r3[1] - r2[1], r3[2] - r2[2], r3[3] - r2[3]]);
        Self { planes }
    }

    pub fn sphere_visible(&self, center: V3, radius: f32) -> bool {
        for p in &self.planes {
            if p[0] * center.x + p[1] * center.y + p[2] * center.z + p[3] < -radius {
                return false;
            }
        }
        true
    }
}

/// Model matrix for a yaw rotation around +Y followed by a translation.
pub fn model_matrix(offset: V3, yaw: f32, scale: f32) -> [f32; 16] {
    let (s, c) = yaw.sin_cos();
    let (s, c) = (s * scale, c * scale);
    [
        c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, offset.x, offset.y, offset.z, 1.0,
    ]
}

/// Small deterministic pseudo-random helpers shared by placement code.
pub fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

pub fn rand01(seed: u32) -> f32 {
    (hash_u32(seed) & 0x00ff_ffff) as f32 / 16_777_215.0
}

pub fn rand_range(seed: u32, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * rand01(seed)
}
