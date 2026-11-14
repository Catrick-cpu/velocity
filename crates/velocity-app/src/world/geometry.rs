//! CPU-side geometry construction: primitives, vegetation, buildings and the
//! cyclist avatar. Everything is generated procedurally, so VELOCITY ships no
//! third-party meshes or textures and stays free of asset licensing questions.

use super::math::{rand01, v3, V3};

/// Vertex layout shared by every world shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 3],
}

impl Vertex {
    pub fn new(pos: V3, normal: V3, uv: [f32; 2], color: [f32; 3]) -> Self {
        Self {
            pos: pos.to_array(),
            normal: normal.to_array(),
            uv,
            color,
        }
    }
}

pub const VERTEX_FLOATS: usize = 11;

/// Per-instance data: position + uniform scale, then yaw + colour variation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Instance {
    pub pos_scale: [f32; 4],
    pub rot_tint: [f32; 4],
}

impl Instance {
    pub fn new(pos: V3, scale: f32, yaw: f32, tint: f32) -> Self {
        Self {
            pos_scale: [pos.x, pos.y, pos.z, scale],
            rot_tint: [yaw, tint, 0.0, 0.0],
        }
    }
}

/// Convert an sRGB-ish colour literal to the linear space the shaders light in.
pub fn rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    fn channel(v: u8) -> f32 {
        let c = v as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    [channel(r), channel(g), channel(b)]
}

#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn tri_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn push_tri(&mut self, a: Vertex, b: Vertex, c: Vertex) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&[a, b, c]);
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    /// Quad `a-b-c-d` given counter-clockwise when seen from the front.
    pub fn push_quad(&mut self, a: Vertex, b: Vertex, c: Vertex, d: Vertex) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&[a, b, c, d]);
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Append another mesh, offsetting its indices.
    pub fn append(&mut self, other: &MeshData) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    pub fn transform(&mut self, offset: V3, yaw: f32) {
        for v in &mut self.vertices {
            let p = v3(v.pos[0], v.pos[1], v.pos[2]).rotate_y(yaw);
            v.pos = (p + offset).to_array();
            let n = v3(v.normal[0], v.normal[1], v.normal[2]).rotate_y(yaw);
            v.normal = n.to_array();
        }
    }
}

fn face_vertex(p: V3, normal: V3, color: [f32; 3], uv: [f32; 2]) -> Vertex {
    Vertex::new(p, normal, uv, color)
}

/// Axis-aligned box, useful for buildings and bike parts.
pub fn add_box(mesh: &mut MeshData, center: V3, half: V3, color: [f32; 3]) {
    let (x, y, z) = (half.x, half.y, half.z);
    let c = center;
    let corners = [
        c + v3(-x, -y, -z),
        c + v3(x, -y, -z),
        c + v3(x, y, -z),
        c + v3(-x, y, -z),
        c + v3(-x, -y, z),
        c + v3(x, -y, z),
        c + v3(x, y, z),
        c + v3(-x, y, z),
    ];
    let faces: [(usize, usize, usize, usize, V3); 6] = [
        (0, 1, 2, 3, v3(0.0, 0.0, -1.0)),
        (4, 5, 6, 7, v3(0.0, 0.0, 1.0)),
        (0, 4, 7, 3, v3(-1.0, 0.0, 0.0)),
        (1, 5, 6, 2, v3(1.0, 0.0, 0.0)),
        (3, 2, 6, 7, v3(0.0, 1.0, 0.0)),
        (0, 1, 5, 4, v3(0.0, -1.0, 0.0)),
    ];
    for (a, b, cc, d, n) in faces {
        mesh.push_quad(
            face_vertex(corners[a], n, color, [0.0, 0.0]),
            face_vertex(corners[b], n, color, [1.0, 0.0]),
            face_vertex(corners[cc], n, color, [1.0, 1.0]),
            face_vertex(corners[d], n, color, [0.0, 1.0]),
        );
    }
}

/// Cylinder between two points (trunk, frame tube, limb).
pub fn add_cylinder(
    mesh: &mut MeshData,
    a: V3,
    b: V3,
    radius_a: f32,
    radius_b: f32,
    sides: usize,
    color: [f32; 3],
) {
    let axis = (b - a).normalized();
    if axis.length() < 0.5 {
        return;
    }
    let up = if axis.y.abs() > 0.95 {
        v3(1.0, 0.0, 0.0)
    } else {
        V3::UP
    };
    let u = axis.cross(up).normalized();
    let v = axis.cross(u).normalized();
    for i in 0..sides {
        let t0 = i as f32 / sides as f32 * std::f32::consts::TAU;
        let t1 = (i + 1) as f32 / sides as f32 * std::f32::consts::TAU;
        let d0 = u * t0.cos() + v * t0.sin();
        let d1 = u * t1.cos() + v * t1.sin();
        let n0 = d0;
        let n1 = d1;
        mesh.push_quad(
            face_vertex(a + d0 * radius_a, n0, color, [t0, 0.0]),
            face_vertex(b + d0 * radius_b, n0, color, [t0, 1.0]),
            face_vertex(b + d1 * radius_b, n1, color, [t1, 1.0]),
            face_vertex(a + d1 * radius_a, n1, color, [t1, 0.0]),
        );
        // End caps keep tubes solid when seen from below.
        mesh.push_tri(
            face_vertex(a, -axis, color, [0.0, 0.0]),
            face_vertex(a + d0 * radius_a, -axis, color, [0.0, 0.0]),
            face_vertex(a + d1 * radius_a, -axis, color, [0.0, 0.0]),
        );
        mesh.push_tri(
            face_vertex(b, axis, color, [0.0, 0.0]),
            face_vertex(b + d1 * radius_b, axis, color, [0.0, 0.0]),
            face_vertex(b + d0 * radius_b, axis, color, [0.0, 0.0]),
        );
    }
}

/// Closed cone with its base at `base`.
pub fn add_cone(mesh: &mut MeshData, base: V3, radius: f32, height: f32, sides: usize, color: [f32; 3]) {
    let tip = base + v3(0.0, height, 0.0);
    for i in 0..sides {
        let t0 = i as f32 / sides as f32 * std::f32::consts::TAU;
        let t1 = (i + 1) as f32 / sides as f32 * std::f32::consts::TAU;
        let p0 = base + v3(t0.cos() * radius, 0.0, t0.sin() * radius);
        let p1 = base + v3(t1.cos() * radius, 0.0, t1.sin() * radius);
        let n = ((p0 + p1) * 0.5 - base).normalized();
        mesh.push_tri(
            face_vertex(p0, n, color, [0.0, 0.0]),
            face_vertex(p1, n, color, [0.0, 0.0]),
            face_vertex(tip, v3(n.x * 0.3, 1.0, n.z * 0.3), color, [0.5, 1.0]),
        );
        mesh.push_tri(
            face_vertex(base, v3(0.0, -1.0, 0.0), color, [0.0, 0.0]),
            face_vertex(p1, v3(0.0, -1.0, 0.0), color, [0.0, 0.0]),
            face_vertex(p0, v3(0.0, -1.0, 0.0), color, [0.0, 0.0]),
        );
    }
}

/// Low-poly ellipsoid used for tree crowns and the rider's head.
pub fn add_ellipsoid(
    mesh: &mut MeshData,
    center: V3,
    radii: V3,
    rings: usize,
    sides: usize,
    color: [f32; 3],
) {
    let rings = rings.max(2);
    let sides = sides.max(3);
    let point = |ring: usize, side: usize| -> V3 {
        let phi = ring as f32 / rings as f32 * std::f32::consts::PI;
        let theta = side as f32 / sides as f32 * std::f32::consts::TAU;
        v3(
            phi.sin() * theta.cos() * radii.x,
            phi.cos() * radii.y,
            phi.sin() * theta.sin() * radii.z,
        ) + center
    };
    for ring in 0..rings {
        for side in 0..sides {
            let p0 = point(ring, side);
            let p1 = point(ring, side + 1);
            let p2 = point(ring + 1, side + 1);
            let p3 = point(ring + 1, side);
            let n0 = (p0 - center).normalized();
            let n1 = (p1 - center).normalized();
            let n2 = (p2 - center).normalized();
            let n3 = (p3 - center).normalized();
            mesh.push_quad(
                face_vertex(p0, n0, color, [0.0, 0.0]),
                face_vertex(p1, n1, color, [1.0, 0.0]),
                face_vertex(p2, n2, color, [1.0, 1.0]),
                face_vertex(p3, n3, color, [0.0, 1.0]),
            );
        }
    }
}

/// Rotate a local point around Y and translate it (building placement).
fn local(base: V3, yaw: f32, point: V3) -> V3 {
    point.rotate_y(yaw) + base
}

/// Gabled house: walls, gable ends and a pitched roof. Local X is the ridge.
pub fn add_house(
    mesh: &mut MeshData,
    base: V3,
    yaw: f32,
    width: f32,
    depth: f32,
    wall_h: f32,
    roof_h: f32,
    wall: [f32; 3],
    roof: [f32; 3],
) {
    let hw = width * 0.5;
    let hd = depth * 0.5;
    let p = |x: f32, y: f32, z: f32| local(base, yaw, v3(x, y, z));
    let wall_normal = v3(0.0, 0.0, 1.0).rotate_y(yaw);
    let side_normal = v3(1.0, 0.0, 0.0).rotate_y(yaw);
    // Walls (no bottom face needed).
    mesh.push_quad(
        face_vertex(p(-hw, 0.0, hd), wall_normal, wall, [0.0, 0.0]),
        face_vertex(p(hw, 0.0, hd), wall_normal, wall, [1.0, 0.0]),
        face_vertex(p(hw, wall_h, hd), wall_normal, wall, [1.0, 1.0]),
        face_vertex(p(-hw, wall_h, hd), wall_normal, wall, [0.0, 1.0]),
    );
    mesh.push_quad(
        face_vertex(p(hw, 0.0, -hd), -wall_normal, wall, [0.0, 0.0]),
        face_vertex(p(-hw, 0.0, -hd), -wall_normal, wall, [1.0, 0.0]),
        face_vertex(p(-hw, wall_h, -hd), -wall_normal, wall, [1.0, 1.0]),
        face_vertex(p(hw, wall_h, -hd), -wall_normal, wall, [0.0, 1.0]),
    );
    mesh.push_quad(
        face_vertex(p(hw, 0.0, hd), side_normal, wall, [0.0, 0.0]),
        face_vertex(p(hw, 0.0, -hd), side_normal, wall, [1.0, 0.0]),
        face_vertex(p(hw, wall_h, -hd), side_normal, wall, [1.0, 1.0]),
        face_vertex(p(hw, wall_h, hd), side_normal, wall, [0.0, 1.0]),
    );
    mesh.push_quad(
        face_vertex(p(-hw, 0.0, -hd), -side_normal, wall, [0.0, 0.0]),
        face_vertex(p(-hw, 0.0, hd), -side_normal, wall, [1.0, 0.0]),
        face_vertex(p(-hw, wall_h, hd), -side_normal, wall, [1.0, 1.0]),
        face_vertex(p(-hw, wall_h, -hd), -side_normal, wall, [0.0, 1.0]),
    );
    // Gable triangles.
    mesh.push_tri(
        face_vertex(p(-hw, wall_h, hd), wall_normal, wall, [0.0, 1.0]),
        face_vertex(p(hw, wall_h, hd), wall_normal, wall, [1.0, 1.0]),
        face_vertex(p(0.0, wall_h + roof_h, hd), wall_normal, wall, [0.5, 1.4]),
    );
    mesh.push_tri(
        face_vertex(p(hw, wall_h, -hd), -wall_normal, wall, [0.0, 1.0]),
        face_vertex(p(-hw, wall_h, -hd), -wall_normal, wall, [1.0, 1.0]),
        face_vertex(p(0.0, wall_h + roof_h, -hd), -wall_normal, wall, [0.5, 1.4]),
    );
    // Roof planes, slightly overhanging.
    let eave = 0.35;
    let ridge = v3(0.0, wall_h + roof_h, 0.0);
    let roof_normal_a = v3(0.0, 1.0, -roof_h / hd.max(0.1)).normalized().rotate_y(yaw);
    let roof_normal_b = v3(0.0, 1.0, roof_h / hd.max(0.1)).normalized().rotate_y(yaw);
    mesh.push_quad(
        face_vertex(p(-hw - eave, wall_h, hd + eave), roof_normal_b, roof, [0.0, 0.0]),
        face_vertex(p(hw + eave, wall_h, hd + eave), roof_normal_b, roof, [1.0, 0.0]),
        face_vertex(p(hw + eave, ridge.y, 0.0), roof_normal_b, roof, [1.0, 1.0]),
        face_vertex(p(-hw - eave, ridge.y, 0.0), roof_normal_b, roof, [0.0, 1.0]),
    );
    mesh.push_quad(
        face_vertex(p(hw + eave, wall_h, -hd - eave), roof_normal_a, roof, [0.0, 0.0]),
        face_vertex(p(-hw - eave, wall_h, -hd - eave), roof_normal_a, roof, [1.0, 0.0]),
        face_vertex(p(-hw - eave, ridge.y, 0.0), roof_normal_a, roof, [1.0, 1.0]),
        face_vertex(p(hw + eave, ridge.y, 0.0), roof_normal_a, roof, [0.0, 1.0]),
    );
}

/// Church with a square tower: a village landmark.
pub fn add_church(mesh: &mut MeshData, base: V3, yaw: f32, wall: [f32; 3], roof: [f32; 3]) {
    add_house(mesh, base, yaw, 12.0, 7.0, 7.0, 3.5, wall, roof);
    let tower_base = base + v3(0.0, 0.0, -6.5).rotate_y(yaw);
    let mut tower = MeshData::new();
    add_box(&mut tower, v3(0.0, 9.0, 0.0), v3(2.0, 9.0, 2.0), wall);
    add_cone(&mut tower, v3(0.0, 18.0, 0.0), 2.6, 6.0, 4, roof);
    tower.transform(tower_base, yaw);
    mesh.append(&tower);
}

/// Conifer: a tapered trunk with three stacked crowns.
pub fn conifer_mesh() -> MeshData {
    let mut m = MeshData::new();
    let bark = rgb(92, 66, 48);
    let needles = rgb(38, 74, 44);
    add_cylinder(&mut m, v3(0.0, 0.0, 0.0), v3(0.0, 6.2, 0.0), 0.16, 0.07, 5, bark);
    add_cone(&mut m, v3(0.0, 1.3, 0.0), 1.55, 2.6, 7, needles);
    add_cone(&mut m, v3(0.0, 2.9, 0.0), 1.2, 2.4, 7, needles);
    add_cone(&mut m, v3(0.0, 4.3, 0.0), 0.82, 2.2, 7, needles);
    m
}

/// Broadleaf: trunk with two irregular blobs of foliage.
pub fn broadleaf_mesh() -> MeshData {
    let mut m = MeshData::new();
    let bark = rgb(96, 76, 58);
    let leaves = rgb(58, 96, 42);
    let leaves_light = rgb(74, 112, 46);
    add_cylinder(&mut m, v3(0.0, 0.0, 0.0), v3(0.0, 2.6, 0.0), 0.2, 0.12, 5, bark);
    add_ellipsoid(&mut m, v3(0.0, 3.8, 0.0), v3(1.75, 1.6, 1.75), 3, 7, leaves);
    add_ellipsoid(&mut m, v3(0.45, 4.9, 0.25), v3(1.15, 1.2, 1.15), 3, 6, leaves_light);
    add_ellipsoid(&mut m, v3(-0.6, 4.4, -0.35), v3(0.95, 1.0, 0.95), 2, 6, leaves);
    m
}

/// Two crossed alpha-tested quads for grass tufts.
pub fn grass_mesh() -> MeshData {
    let mut m = MeshData::new();
    let c = [1.0f32, 1.0, 1.0];
    for i in 0..2 {
        let a = i as f32 * std::f32::consts::FRAC_PI_2;
        let d = v3(a.cos(), 0.0, a.sin());
        let p0 = d * -0.55;
        let p1 = d * 0.55;
        m.push_quad(
            face_vertex(p0, V3::UP, c, [0.0, 0.0]),
            face_vertex(p1, V3::UP, c, [1.0, 0.0]),
            face_vertex(p1 + v3(0.0, 0.7, 0.0), V3::UP, c, [1.0, 1.0]),
            face_vertex(p0 + v3(0.0, 0.7, 0.0), V3::UP, c, [0.0, 1.0]),
        );
    }
    m
}

/// Build the animated cyclist for a given pedal phase. Local +Z is forward,
/// the bike sits on y = 0. The whole mesh is regenerated each frame because the
/// silhouette is small (≈1.5k triangles) and exact animation reads better than
/// skinning several rigged parts.
pub struct RiderLook {
    pub jersey: [f32; 3],
    pub helmet: [f32; 3],
    pub shorts: [f32; 3],
    pub skin: [f32; 3],
}

impl Default for RiderLook {
    fn default() -> Self {
        Self {
            jersey: rgb(26, 30, 38),
            helmet: rgb(200, 241, 53),
            shorts: rgb(18, 22, 28),
            skin: rgb(196, 152, 118),
        }
    }
}

pub fn build_cyclist(crank_angle: f32, lean: f32, wheel_angle: f32, look: &RiderLook) -> MeshData {
    let mut m = MeshData::new();
    let frame = rgb(24, 26, 30);
    let accent = rgb(200, 241, 53);
    let tyre = rgb(20, 20, 22);
    let rim = rgb(70, 74, 80);
    let stem = rgb(60, 62, 68);

    // Wheels: 0.34 m radius, 1.1 m wheelbase.
    for z in [-0.55f32, 0.55] {
        let center = v3(0.0, 0.34, z);
        let mut wheel = |radius: f32, width: f32, color: [f32; 3], sides: usize| {
            for i in 0..sides {
                let a0 = i as f32 / sides as f32 * std::f32::consts::TAU + wheel_angle;
                let a1 = (i + 1) as f32 / sides as f32 * std::f32::consts::TAU + wheel_angle;
                let dir0 = v3(a0.cos(), a0.sin(), 0.0);
                let dir1 = v3(a1.cos(), a1.sin(), 0.0);
                let n0 = dir0;
                let n1 = dir1;
                m.push_quad(
                    face_vertex(center + dir0 * radius - v3(0.0, 0.0, width), n0, color, [0.0, 0.0]),
                    face_vertex(center + dir0 * radius + v3(0.0, 0.0, width), n0, color, [1.0, 0.0]),
                    face_vertex(center + dir1 * radius + v3(0.0, 0.0, width), n1, color, [1.0, 1.0]),
                    face_vertex(center + dir1 * radius - v3(0.0, 0.0, width), n1, color, [0.0, 1.0]),
                );
            }
        };
        wheel(0.335, 0.032, tyre, 20);
        wheel(0.29, 0.012, rim, 20);
        // A few spokes make rotation visible.
        for k in 0..6 {
            let a = wheel_angle + k as f32 * std::f32::consts::PI / 3.0;
            let dir = v3(a.cos(), a.sin(), 0.0);
            add_cylinder(&mut m, center, center + dir * 0.29, 0.008, 0.008, 3, rim);
        }
        add_cylinder(&mut m, center - v3(0.0, 0.0, 0.05), center + v3(0.0, 0.0, 0.05), 0.03, 0.03, 6, stem);
    }

    // Frame triangle.
    let rear_hub = v3(0.0, 0.34, -0.55);
    let front_hub = v3(0.0, 0.34, 0.55);
    let bottom_bracket = v3(0.0, 0.3, 0.02);
    let seat_top = v3(0.0, 0.95, -0.18);
    let head_top = v3(0.0, 0.92, 0.4);
    add_cylinder(&mut m, bottom_bracket, seat_top, 0.03, 0.025, 6, frame);
    add_cylinder(&mut m, bottom_bracket, head_top, 0.035, 0.03, 6, frame);
    add_cylinder(&mut m, seat_top, head_top, 0.028, 0.028, 6, frame);
    add_cylinder(&mut m, rear_hub, seat_top, 0.02, 0.02, 5, frame);
    add_cylinder(&mut m, rear_hub, bottom_bracket, 0.022, 0.022, 5, frame);
    add_cylinder(&mut m, front_hub, head_top, 0.022, 0.022, 5, frame);
    add_cylinder(&mut m, head_top, v3(0.0, 1.0, 0.47), 0.026, 0.026, 5, stem);
    // Handlebar (drops) and saddle.
    add_cylinder(&mut m, v3(-0.21, 1.0, 0.47), v3(0.21, 1.0, 0.47), 0.018, 0.018, 6, stem);
    add_cylinder(&mut m, v3(-0.21, 1.0, 0.47), v3(-0.21, 0.95, 0.55), 0.018, 0.018, 5, stem);
    add_cylinder(&mut m, v3(0.21, 1.0, 0.47), v3(0.21, 0.95, 0.55), 0.018, 0.018, 5, stem);
    add_box(&mut m, v3(0.0, 1.02, -0.2), v3(0.055, 0.02, 0.13), frame);
    add_cylinder(&mut m, v3(0.0, 0.95, -0.18), v3(0.0, 1.01, -0.2), 0.02, 0.02, 5, stem);

    // Chainring and crank arms.
    let mut crank = |phase: f32, side: f32| {
        let dir = v3(0.0, phase.sin(), phase.cos());
        add_cylinder(
            &mut m,
            bottom_bracket + v3(side, 0.0, 0.0),
            bottom_bracket + v3(side, 0.0, 0.0) + dir * 0.17,
            0.012,
            0.012,
            4,
            accent,
        );
        let pedal = bottom_bracket + v3(side, 0.0, 0.0) + dir * 0.17;
        add_box(&mut m, pedal + v3(side * 0.03, 0.0, 0.0), v3(0.045, 0.012, 0.03), tyre);
        pedal
    };
    let pedal_left = crank(crank_angle, -0.09);
    let pedal_right = crank(crank_angle + std::f32::consts::PI, 0.09);
    // Chainring.
    for i in 0..16 {
        let a0 = i as f32 / 16.0 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / 16.0 * std::f32::consts::TAU;
        let p0 = bottom_bracket + v3(0.0, a0.sin(), a0.cos()) * 0.11;
        let p1 = bottom_bracket + v3(0.0, a1.sin(), a1.cos()) * 0.11;
        m.push_quad(
            face_vertex(p0, v3(1.0, 0.0, 0.0), rim, [0.0, 0.0]),
            face_vertex(p1, v3(1.0, 0.0, 0.0), rim, [1.0, 0.0]),
            face_vertex(p1 + v3(0.006, 0.0, 0.0), v3(1.0, 0.0, 0.0), rim, [1.0, 1.0]),
            face_vertex(p0 + v3(0.006, 0.0, 0.0), v3(1.0, 0.0, 0.0), rim, [0.0, 1.0]),
        );
    }

    // Rider. Hips sit over the saddle, torso leans onto the bars.
    let hip = v3(0.0, 1.08, -0.16);
    let shoulder = v3(0.0, 1.42, 0.16);
    let torso_dir = (shoulder - hip).normalized();
    let torso_right = torso_dir.cross(V3::UP).normalized();
    let torso_normal = torso_dir.cross(torso_right).normalized();
    // Simple tapered torso from four quads + caps.
    let ring = |center: V3, radius_x: f32, radius_y: f32| -> [V3; 4] {
        [
            center + torso_right * radius_x,
            center + torso_normal * radius_y,
            center - torso_right * radius_x,
            center - torso_normal * radius_y,
        ]
    };
    let hip_ring = ring(hip, 0.17, 0.12);
    let shoulder_ring = ring(shoulder, 0.21, 0.13);
    for i in 0..4 {
        let j = (i + 1) % 4;
        m.push_quad(
            face_vertex(hip_ring[i], (hip_ring[i] - hip).normalized(), look.jersey, [0.0, 0.0]),
            face_vertex(hip_ring[j], (hip_ring[j] - hip).normalized(), look.jersey, [1.0, 0.0]),
            face_vertex(shoulder_ring[j], (shoulder_ring[j] - shoulder).normalized(), look.jersey, [1.0, 1.0]),
            face_vertex(shoulder_ring[i], (shoulder_ring[i] - shoulder).normalized(), look.jersey, [0.0, 1.0]),
        );
    }
    add_box(&mut m, hip, v3(0.16, 0.09, 0.12), look.shorts);
    // Neck + head + helmet.
    let head = shoulder + v3(0.0, 0.1, 0.13);
    add_cylinder(&mut m, shoulder, head, 0.055, 0.055, 5, look.skin);
    add_ellipsoid(&mut m, head, v3(0.1, 0.115, 0.12), 3, 7, look.skin);
    add_ellipsoid(&mut m, head + v3(0.0, 0.045, -0.01), v3(0.115, 0.085, 0.135), 2, 7, look.helmet);

    // Arms: shoulder → elbow → hands on the drops.
    let hand_left = v3(-0.21, 1.02, 0.5);
    let hand_right = v3(0.21, 1.02, 0.5);
    for (side, hand) in [(-1.0f32, hand_left), (1.0, hand_right)] {
        let shoulder_p = shoulder + v3(side * 0.19, 0.0, 0.0);
        let elbow = shoulder_p.lerp(hand, 0.5) + v3(side * 0.04, -0.1, 0.0);
        add_cylinder(&mut m, shoulder_p, elbow, 0.048, 0.04, 5, look.jersey);
        add_cylinder(&mut m, elbow, hand, 0.038, 0.032, 5, look.skin);
    }

    // Legs with a two-bone solve for thigh and shin.
    for (side, pedal) in [(-1.0f32, pedal_left), (1.0, pedal_right)] {
        let hip_p = hip + v3(side * 0.1, -0.02, 0.0);
        let ankle = pedal + v3(side * 0.02, 0.06, -0.02);
        let thigh_len = 0.46;
        let shin_len = 0.46;
        let delta = ankle - hip_p;
        let dist = delta.length().clamp(0.1, thigh_len + shin_len - 0.01);
        let dir = delta.normalized();
        // Law of cosines, knee pushed forward (+Z).
        let cos_a = ((thigh_len * thigh_len + dist * dist - shin_len * shin_len)
            / (2.0 * thigh_len * dist))
            .clamp(-1.0, 1.0);
        let angle = cos_a.acos();
        // Bend the knee forward: project +Z onto the plane perpendicular to the
        // hip→ankle direction and rotate the thigh towards it.
        let mut bend = v3(0.0, 0.0, 1.0) - dir * dir.z;
        if bend.length() < 0.01 {
            bend = v3(0.0, 1.0, 0.0);
        }
        let bend = bend.normalized();
        let thigh_dir = (dir * angle.cos() + bend * angle.sin()).normalized();
        let knee = hip_p + thigh_dir * thigh_len;
        add_cylinder(&mut m, hip_p, knee, 0.085, 0.06, 6, look.shorts);
        add_cylinder(&mut m, knee, ankle, 0.055, 0.038, 6, look.skin);
        add_box(&mut m, ankle + v3(0.0, -0.02, 0.05), v3(0.04, 0.03, 0.09), rim);
    }

    // Lean into the corner around the forward axis.
    if lean.abs() > 0.001 {
        let pivot = v3(0.0, 0.34, 0.0);
        let axis = v3(0.0, 0.0, 1.0);
        for v in &mut m.vertices {
            let p = v3(v.pos[0], v.pos[1], v.pos[2]) - pivot;
            let rotated = p * lean.cos() + axis.cross(p) * lean.sin() + axis * (axis.dot(p) * (1.0 - lean.cos()));
            v.pos = (rotated + pivot).to_array();
            let n = v3(v.normal[0], v.normal[1], v.normal[2]);
            let rn = n * lean.cos() + axis.cross(n) * lean.sin() + axis * (axis.dot(n) * (1.0 - lean.cos()));
            v.normal = rn.to_array();
        }
    }

    m
}

/// Deterministic tree placement for a terrain chunk cell.
pub fn tree_tint(seed: u32) -> f32 {
    rand01(seed)
}
