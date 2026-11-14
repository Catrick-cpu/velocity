//! OpenGL renderer for the 3D world.
//!
//! Design goals, in order: stable frame pacing on integrated graphics, real
//! elevation and road geometry, and a coherent, atmospheric look. The renderer
//! keeps a small working set of terrain chunks around the rider, draws repeated
//! objects instanced, and renders into a resolution-scaled offscreen buffer so
//! quality can adapt without touching the UI's crisp text.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::PaintCallbackInfo;
use eframe::egui_glow::Painter;
use eframe::glow::{self, HasContext};
use velocity_core::route::Route;

use super::dem::{self, DemLoader, DemMessage, DemStatus};
use super::geometry::{self, Instance, MeshData, Vertex};
use super::math::{self, v3, Frustum, V3};
use super::shaders;
use super::terrain::{RoadSection, Terrain, Village, CHUNK_SIZE};
use super::{CameraMode, Quality, WorldInput, WorldPrefs, WorldStats};

const FOV_Y: f32 = std::f32::consts::PI * 62.0 / 180.0;

struct GlProgram {
    program: glow::Program,
    uniforms: HashMap<&'static str, Option<glow::UniformLocation>>,
}

impl GlProgram {
    unsafe fn new(
        gl: &glow::Context,
        vs_src: &str,
        fs_src: &str,
        names: &[&'static str],
    ) -> Result<Self, String> {
        let vs = compile(gl, glow::VERTEX_SHADER, vs_src)?;
        let fs = compile(gl, glow::FRAGMENT_SHADER, fs_src)?;
        let program = gl.create_program()?;
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.bind_attrib_location(program, 0, "a_pos");
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            return Err(format!("link failed: {log}"));
        }
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        let mut uniforms = HashMap::new();
        for name in names {
            uniforms.insert(*name, gl.get_uniform_location(program, name));
        }
        Ok(Self { program, uniforms })
    }

    unsafe fn bind(&self, gl: &glow::Context) {
        gl.use_program(Some(self.program));
    }

    fn loc(&self, name: &str) -> Option<&glow::UniformLocation> {
        self.uniforms.get(name).and_then(|l| l.as_ref())
    }

    unsafe fn mat4(&self, gl: &glow::Context, name: &str, value: &[f32; 16]) {
        gl.uniform_matrix_4_f32_slice(self.loc(name), false, value);
    }

    unsafe fn vec3(&self, gl: &glow::Context, name: &str, value: V3) {
        gl.uniform_3_f32(self.loc(name), value.x, value.y, value.z);
    }

    unsafe fn f32(&self, gl: &glow::Context, name: &str, value: f32) {
        gl.uniform_1_f32(self.loc(name), value);
    }

    unsafe fn i32(&self, gl: &glow::Context, name: &str, value: i32) {
        gl.uniform_1_i32(self.loc(name), value);
    }

    unsafe fn vec2(&self, gl: &glow::Context, name: &str, x: f32, y: f32) {
        gl.uniform_2_f32(self.loc(name), x, y);
    }
}

unsafe fn compile(gl: &glow::Context, stage: u32, src: &str) -> Result<glow::Shader, String> {
    let shader = gl.create_shader(stage)?;
    gl.shader_source(shader, src);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        return Err(format!("shader compile failed: {log}"));
    }
    Ok(shader)
}

struct GpuMesh {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    count: i32,
}

impl GpuMesh {
    unsafe fn upload(gl: &glow::Context, mesh: &MeshData) -> Result<Self, String> {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        let ebo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&mesh.vertices), glow::STATIC_DRAW);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
        gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, as_bytes(&mesh.indices), glow::STATIC_DRAW);
        let stride = (geometry::VERTEX_FLOATS * 4) as i32;
        let float = 4i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 3 * float);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 6 * float);
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_f32(3, 3, glow::FLOAT, false, stride, 8 * float);
        gl.bind_vertex_array(None);
        Ok(Self {
            vao,
            vbo,
            ebo,
            count: mesh.indices.len() as i32,
        })
    }

    unsafe fn draw(&self, gl: &glow::Context) {
        if self.count == 0 {
            return;
        }
        gl.bind_vertex_array(Some(self.vao));
        gl.draw_elements(glow::TRIANGLES, self.count, glow::UNSIGNED_INT, 0);
    }

    unsafe fn draw_range(&self, gl: &glow::Context, first: u32, count: u32) {
        if count == 0 {
            return;
        }
        gl.bind_vertex_array(Some(self.vao));
        gl.draw_elements(
            glow::TRIANGLES,
            count as i32,
            glow::UNSIGNED_INT,
            (first * 4) as i32,
        );
    }

    unsafe fn destroy(&self, gl: &glow::Context) {
        gl.delete_vertex_array(self.vao);
        gl.delete_buffer(self.vbo);
        gl.delete_buffer(self.ebo);
    }
}

struct InstancedMesh {
    gpu: GpuMesh,
    instances: glow::Buffer,
}

impl InstancedMesh {
    unsafe fn new(gl: &glow::Context, mesh: &MeshData) -> Result<Self, String> {
        let gpu = GpuMesh::upload(gl, mesh)?;
        let instances = gl.create_buffer()?;
        gl.bind_vertex_array(Some(gpu.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(instances));
        gl.enable_vertex_attrib_array(4);
        gl.vertex_attrib_pointer_f32(4, 4, glow::FLOAT, false, 32, 0);
        gl.vertex_attrib_divisor(4, 1);
        gl.enable_vertex_attrib_array(5);
        gl.vertex_attrib_pointer_f32(5, 4, glow::FLOAT, false, 32, 16);
        gl.vertex_attrib_divisor(5, 1);
        gl.bind_vertex_array(None);
        Ok(Self { gpu, instances })
    }

    unsafe fn set_instances(&self, gl: &glow::Context, instances: &[Instance]) {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(instances), glow::DYNAMIC_DRAW);
    }

    unsafe fn draw(&self, gl: &glow::Context, count: usize) {
        if count == 0 || self.gpu.count == 0 {
            return;
        }
        gl.bind_vertex_array(Some(self.gpu.vao));
        gl.draw_elements_instanced(
            glow::TRIANGLES,
            self.gpu.count,
            glow::UNSIGNED_INT,
            0,
            count as i32,
        );
    }
}

struct DynamicMesh {
    gpu: GpuMesh,
}

impl DynamicMesh {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let empty = MeshData::new();
        Ok(Self {
            gpu: GpuMesh::upload(gl, &empty)?,
        })
    }

    unsafe fn upload(&mut self, gl: &glow::Context, mesh: &MeshData) {
        gl.bind_vertex_array(Some(self.gpu.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.gpu.vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&mesh.vertices), glow::DYNAMIC_DRAW);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.gpu.ebo));
        gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, as_bytes(&mesh.indices), glow::DYNAMIC_DRAW);
        gl.bind_vertex_array(None);
        self.gpu.count = mesh.indices.len() as i32;
    }
}

fn as_bytes<T>(slice: &[T]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            slice.as_ptr() as *const u8,
            std::mem::size_of_val(slice),
        )
    }
}

struct Textures {
    grass: glow::Texture,
    rock: glow::Texture,
    asphalt: glow::Texture,
    detail: glow::Texture,
    ripple: glow::Texture,
    alpha: glow::Texture,
}

struct Programs {
    terrain: GlProgram,
    road: GlProgram,
    mesh: GlProgram,
    water: GlProgram,
    sky: GlProgram,
    post: GlProgram,
    /// Soft contact shadow under the rider.
    shadow: GlProgram,
    /// Depth-only pass for the sun shadow map.
    depth: GlProgram,
}

struct SceneFbo {
    fbo: glow::Framebuffer,
    color: glow::Texture,
    depth: glow::Renderbuffer,
    width: i32,
    height: i32,
}

impl SceneFbo {
    unsafe fn new(gl: &glow::Context, width: i32, height: i32) -> Result<Self, String> {
        let fbo = gl.create_framebuffer()?;
        let color = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(color));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width,
            height,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        let depth = gl.create_renderbuffer()?;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
        gl.renderbuffer_storage(
            glow::RENDERBUFFER,
            glow::DEPTH_COMPONENT24,
            width,
            height,
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(color),
            0,
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_ATTACHMENT,
            glow::RENDERBUFFER,
            Some(depth),
        );
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            return Err("world framebuffer is incomplete".to_string());
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        Ok(Self {
            fbo,
            color,
            depth,
            width,
            height,
        })
    }
}

struct ShadowMap {
    fbo: glow::Framebuffer,
    depth: glow::Texture,
    size: i32,
}

impl ShadowMap {
    unsafe fn new(gl: &glow::Context, size: i32) -> Result<Self, String> {
        let fbo = gl.create_framebuffer()?;
        let depth = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(depth));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::DEPTH_COMPONENT24 as i32,
            size,
            size,
            0,
            glow::DEPTH_COMPONENT,
            glow::UNSIGNED_INT,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::DEPTH_ATTACHMENT,
            glow::TEXTURE_2D,
            Some(depth),
            0,
        );
        gl.draw_buffer(glow::NONE);
        gl.read_buffer(glow::NONE);
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            return Err("shadow map framebuffer is incomplete".to_string());
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        Ok(Self { fbo, depth, size })
    }
}

struct Chunk {
    mesh: GpuMesh,
    res: u32,
    /// DEM version the mesh was built from; stale chunks get rebuilt in place.
    version: u64,
    center: V3,
    radius: f32,
    trees_conifer: Vec<Instance>,
    trees_broadleaf: Vec<Instance>,
    grass: Vec<Instance>,
}

struct CameraState {
    eye: V3,
    target: V3,
    initialized: bool,
    yaw_offset: f32,
    pitch_offset: f32,
    distance_scale: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            eye: V3::ZERO,
            target: V3::ZERO,
            initialized: false,
            yaw_offset: 0.0,
            pitch_offset: 0.0,
            distance_scale: 1.0,
        }
    }
}

struct Environment {
    sun_dir: V3,
    sun_color: V3,
    sky_color: V3,
    ground_color: V3,
    zenith: V3,
    horizon: V3,
    fog_color: V3,
    fog_density: f32,
    night: f32,
    exposure: f32,
}

fn environment(hour: f32, cloud: f32, view_distance: f32, wet: f32) -> Environment {
    let day_t = ((hour - 6.0) / 12.0).clamp(0.0, 1.0);
    let elevation = (day_t * std::f32::consts::PI).sin() * 1.12 - 0.04;
    let azimuth = day_t * std::f32::consts::PI * 0.92 + 0.55;
    let sun_dir = v3(
        azimuth.cos() * elevation.cos(),
        elevation.sin().max(-0.35),
        azimuth.sin() * elevation.cos(),
    )
    .normalized();
    let day = (elevation / 0.35).clamp(0.0, 1.0);
    let golden = (1.0 - (elevation / 0.55).clamp(0.0, 1.0)).powf(1.6);
    let sun_color = v3(
        1.0,
        0.95 - golden * 0.34,
        0.88 - golden * 0.55,
    ) * (0.35 + day * 1.45);
    let horizon = v3(0.62, 0.71, 0.82) * (0.32 + day * 0.85)
        + v3(0.55, 0.3, 0.12) * golden * 0.55;
    let zenith = v3(0.13, 0.26, 0.5) * (0.22 + day * 0.9);
    let cloud_dim = 1.0 - cloud * 0.28;
    let horizon = horizon * cloud_dim;
    let zenith = zenith * cloud_dim;
    let sun_color = sun_color * (1.0 - cloud * 0.35);
    let ground = v3(0.16, 0.15, 0.12) * (0.22 + day * 0.9);
    let sky_ambient = zenith * 0.9 + horizon * 0.35;
    let fog_color = horizon * 0.92 + sun_color * 0.05;
    let night = (1.0 - day * 3.0).clamp(0.0, 1.0);
    let exposure = 1.05 + night * 2.4 - cloud * 0.08;
    let fog_density = 1.58 / view_distance.max(120.0) * (1.0 + cloud * 0.35);
    let _ = wet;
    Environment {
        sun_dir,
        sun_color,
        sky_color: sky_ambient,
        ground_color: ground,
        zenith,
        horizon,
        fog_color,
        fog_density,
        night,
        exposure,
    }
}

pub struct WorldRenderer {
    cache_dir: PathBuf,
    programs: Option<Programs>,
    textures: Option<Textures>,
    fbo: Option<SceneFbo>,
    error: Option<String>,
    gpu: String,

    terrain: Option<Terrain>,
    route: Option<Arc<Route>>,
    empty_vao: Option<glow::VertexArray>,
    route_key: Option<(u64, u32, u64)>,
    dem_loader: Option<DemLoader>,
    dem_version: u64,
    dem_done: bool,
    initial_fill: bool,

    chunks: HashMap<(i32, i32), Chunk>,
    road: Option<(GpuMesh, Vec<RoadSection>)>,
    villages: Vec<(GpuMesh, V3, f32)>,
    villages_version: u64,
    water: Option<GpuMesh>,

    conifer: Option<InstancedMesh>,
    broadleaf: Option<InstancedMesh>,
    grass: Option<InstancedMesh>,
    avatar: Option<DynamicMesh>,
    shadow: Option<GpuMesh>,
    shadow_map: Option<ShadowMap>,

    instance_cache: (Vec<Instance>, Vec<Instance>, Vec<Instance>),

    camera: CameraState,
    smooth_distance: f32,
    crank_angle: f32,
    wheel_angle: f32,
    elapsed: f32,
    last_paint: Option<Instant>,
    frame_ema_ms: f32,
    adapt_factor: f32,
    adapt_counter: u32,

    quality: Quality,
}

impl WorldRenderer {
    pub fn new(gl: &glow::Context, cache_dir: PathBuf) -> Self {
        let gpu = unsafe { gl.get_parameter_string(glow::RENDERER) };
        let mut renderer = Self {
            cache_dir,
            programs: None,
            textures: None,
            fbo: None,
            error: None,
            gpu,
            terrain: None,
            route: None,
            empty_vao: None,
            route_key: None,
            dem_loader: None,
            dem_version: 0,
            dem_done: false,
            initial_fill: true,
            chunks: HashMap::new(),
            road: None,
            villages: Vec::new(),
            villages_version: u64::MAX,
            water: None,
            conifer: None,
            broadleaf: None,
            grass: None,
            avatar: None,
            shadow: None,
            shadow_map: None,
            instance_cache: (Vec::new(), Vec::new(), Vec::new()),
            camera: CameraState::default(),
            smooth_distance: 0.0,
            crank_angle: 0.0,
            wheel_angle: 0.0,
            elapsed: 0.0,
            last_paint: None,
            frame_ema_ms: 16.7,
            adapt_factor: 1.0,
            adapt_counter: 0,
            quality: Quality::resolve(super::QualityPreset::Auto, super::GpuClass::Integrated),
        };
        match unsafe { renderer.init_gl(gl) } {
            Ok(()) => {}
            Err(err) => {
                renderer.error = Some(err.clone());
                eprintln!("velocity world: {err}");
            }
        }
        renderer
    }

    unsafe fn init_gl(&mut self, gl: &glow::Context) -> Result<(), String> {
        let programs = Programs {
            terrain: GlProgram::new(
                gl,
                shaders::VS_TERRAIN,
                shaders::FS_TERRAIN,
                &[
                    "u_viewproj",
                    "u_cam",
                    "u_sun_dir",
                    "u_sun_color",
                    "u_sky_color",
                    "u_ground_color",
                    "u_fog_color",
                    "u_fog_density",
                    "u_grass",
                    "u_rock",
                    "u_detail",
                    "u_shadow_map",
                    "u_light_viewproj",
                    "u_shadow_strength",
                    "u_shadow_texel",
                ],
            )?,
            road: GlProgram::new(
                gl,
                shaders::VS_ROAD,
                shaders::FS_ROAD,
                &[
                    "u_viewproj",
                    "u_cam",
                    "u_sun_dir",
                    "u_sun_color",
                    "u_sky_color",
                    "u_ground_color",
                    "u_fog_color",
                    "u_fog_density",
                    "u_wet",
                    "u_asphalt",
                    "u_detail",
                    "u_shadow_map",
                    "u_light_viewproj",
                    "u_shadow_strength",
                    "u_shadow_texel",
                ],
            )?,
            mesh: GlProgram::new(
                gl,
                shaders::VS_MESH,
                shaders::FS_MESH,
                &[
                    "u_viewproj",
                    "u_model",
                    "u_instanced",
                    "u_cam",
                    "u_sun_dir",
                    "u_sun_color",
                    "u_sky_color",
                    "u_ground_color",
                    "u_fog_color",
                    "u_fog_density",
                    "u_specular",
                    "u_alpha_test",
                    "u_alpha",
                    "u_shadow_map",
                    "u_light_viewproj",
                    "u_shadow_strength",
                    "u_shadow_texel",
                ],
            )?,
            water: GlProgram::new(
                gl,
                shaders::VS_WATER,
                shaders::FS_WATER,
                &[
                    "u_viewproj",
                    "u_cam",
                    "u_sun_dir",
                    "u_sun_color",
                    "u_fog_color",
                    "u_fog_density",
                    "u_time",
                    "u_ripple",
                ],
            )?,
            sky: GlProgram::new(
                gl,
                shaders::VS_SKY,
                shaders::FS_SKY,
                &[
                    "u_inv_viewproj",
                    "u_cam",
                    "u_sun_dir",
                    "u_sun_color",
                    "u_zenith",
                    "u_horizon",
                    "u_ground",
                    "u_cloud",
                    "u_time",
                    "u_night",
                    "u_detail",
                ],
            )?,
            post: GlProgram::new(
                gl,
                shaders::VS_POST,
                shaders::FS_POST,
                &["u_scene", "u_texel", "u_exposure", "u_fxaa", "u_vignette"],
            )?,
            shadow: GlProgram::new(
                gl,
                shaders::VS_SHADOW,
                shaders::FS_SHADOW,
                &["u_viewproj", "u_model", "u_strength"],
            )?,
            depth: GlProgram::new(
                gl,
                shaders::VS_SHADOW_DEPTH,
                shaders::FS_SHADOW_DEPTH,
                &["u_light_viewproj", "u_model", "u_instanced"],
            )?,
        };
        let textures = Textures {
            grass: upload_texture(gl, 128, 128, &grass_texture(128), true)?,
            rock: upload_texture(gl, 128, 128, &rock_texture(128), true)?,
            asphalt: upload_texture(gl, 128, 128, &asphalt_texture(128), true)?,
            detail: upload_texture(gl, 128, 128, &normal_texture(128, 0.0), false)?,
            ripple: upload_texture(gl, 128, 128, &normal_texture(128, 7.3), false)?,
            alpha: upload_texture(gl, 64, 64, &blade_texture(64), false)?,
        };
        // One-time sampler bindings.
        programs.terrain.bind(gl);
        programs.terrain.i32(gl, "u_grass", 0);
        programs.terrain.i32(gl, "u_rock", 1);
        programs.terrain.i32(gl, "u_detail", 2);
        programs.terrain.i32(gl, "u_shadow_map", 3);
        programs.road.bind(gl);
        programs.road.i32(gl, "u_asphalt", 0);
        programs.road.i32(gl, "u_detail", 2);
        programs.road.i32(gl, "u_shadow_map", 3);
        programs.mesh.bind(gl);
        programs.mesh.i32(gl, "u_alpha", 0);
        programs.mesh.i32(gl, "u_shadow_map", 3);
        programs.water.bind(gl);
        programs.water.i32(gl, "u_ripple", 0);
        programs.sky.bind(gl);
        programs.sky.i32(gl, "u_detail", 0);
        programs.post.bind(gl);
        programs.post.i32(gl, "u_scene", 0);

        if let Ok(conifer) = InstancedMesh::new(gl, &geometry::conifer_mesh()) {
            self.conifer = Some(conifer);
        }
        if let Ok(broadleaf) = InstancedMesh::new(gl, &geometry::broadleaf_mesh()) {
            self.broadleaf = Some(broadleaf);
        }
        if let Ok(grass) = InstancedMesh::new(gl, &geometry::grass_mesh()) {
            self.grass = Some(grass);
        }
        self.avatar = DynamicMesh::new(gl).ok();
        let mut shadow = MeshData::new();
        let c = [0.0, 0.0, 0.0];
        shadow.push_quad(
            Vertex::new(v3(-0.55, 0.0, -1.15), V3::UP, [0.0, 0.0], c),
            Vertex::new(v3(0.55, 0.0, -1.15), V3::UP, [1.0, 0.0], c),
            Vertex::new(v3(0.55, 0.0, 1.15), V3::UP, [1.0, 1.0], c),
            Vertex::new(v3(-0.55, 0.0, 1.15), V3::UP, [0.0, 1.0], c),
        );
        self.shadow = GpuMesh::upload(gl, &shadow).ok();
        self.empty_vao = gl.create_vertex_array().ok();
        self.shadow_map = match ShadowMap::new(gl, 1024) {
            Ok(map) => Some(map),
            Err(err) => {
                log::warn!("world shadows disabled: {err}");
                None
            }
        };

        self.programs = Some(programs);
        self.textures = Some(textures);
        self.error = None;
        Ok(())
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.camera.yaw_offset = (self.camera.yaw_offset + dx * 0.006).clamp(-2.6, 2.6);
        self.camera.pitch_offset = (self.camera.pitch_offset + dy * 0.004).clamp(-0.5, 1.1);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.camera.distance_scale =
            (self.camera.distance_scale * (1.0 + delta * 0.0015)).clamp(0.45, 2.4);
    }

    fn ensure_route(&mut self, gl: &glow::Context, route: &Arc<Route>) {
        let name_hash = route
            .name
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
            });
        let key = (
            route.stats.distance_m.to_bits(),
            route.points.len() as u32,
            name_hash,
        );
        if self.route_key == Some(key) {
            return;
        }
        self.route_key = Some(key);
        self.route = Some(route.clone());
        self.chunks.clear();
        self.road = None;
        self.villages.clear();
        self.villages_version = u64::MAX;
        self.water = None;
        self.camera.initialized = false;
        self.smooth_distance = 0.0;
        let zoom = dem::choose_zoom(route);
        let mut terrain = Terrain::new(route, zoom, 0x5eed_1234);
        let (mesh, sections) = terrain.build_road();
        unsafe {
            if let Ok(gpu) = GpuMesh::upload(gl, &mesh) {
                self.road = Some((gpu, sections));
            }
        }
        self.dem_loader = None;
        self.dem_version = 0;
        self.dem_done = false;
        self.initial_fill = true;
        if route.is_demo {
            // Demo routes ship an invented elevation profile: building the
            // world from that profile keeps road and terrain coherent without
            // waiting on network tiles that describe a different landscape.
            terrain.dem.status = DemStatus::Demo;
            self.dem_done = true;
        } else {
            terrain.dem.status = DemStatus::Loading;
            self.dem_loader = Some(DemLoader::spawn(route.clone(), self.cache_dir.clone()));
        }
        self.terrain = Some(terrain);
    }

    fn poll_dem(&mut self) {
        let Some(loader) = &self.dem_loader else {
            return;
        };
        loop {
            match loader.rx.try_recv() {
                Ok(DemMessage::Tile {
                    zoom,
                    x,
                    y,
                    heights,
                }) => {
                    if let Some(terrain) = &mut self.terrain {
                        terrain.dem.insert(zoom, x, y, heights);
                        // Recalibrating on every tile would rebuild all chunks
                        // over and over; a few samples per batch is plenty.
                        if terrain.dem.tile_count() % 4 == 0 {
                            terrain.calibrate();
                        }
                    }
                }
                Ok(DemMessage::Done {
                    downloaded,
                    cached,
                    failed,
                }) => {
                    self.dem_done = true;
                    let tiles = self.terrain.as_ref().map(|t| t.dem.tile_count()).unwrap_or(0);
                    log::info!(
                        "world elevation: {tiles} tiles ({downloaded} downloaded, {cached} cached, {failed} failed)"
                    );
                    if let Some(terrain) = &mut self.terrain {
                        terrain.calibrate();
                        terrain.dem.status = if tiles > 0 {
                            DemStatus::Ready
                        } else {
                            DemStatus::Offline
                        };
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.dem_done = true;
                    break;
                }
            }
        }
        let version = self.terrain.as_ref().map(|t| t.dem.version).unwrap_or(0);
        if version != self.dem_version {
            self.dem_version = version;
            // Existing chunks stay visible and are rebuilt in place as new
            // elevation arrives, so the landscape never disappears mid-load.
        }
    }

    fn update_chunks(&mut self, gl: &glow::Context, rider: V3, view_distance: f32, budget: usize) {
        // Temporarily move the terrain out so chunk storage can be mutated
        // while sampling heights from the same structure.
        let Some(terrain) = self.terrain.take() else { return };
        let ccx = (rider.x / CHUNK_SIZE).floor() as i32;
        let ccz = (rider.z / CHUNK_SIZE).floor() as i32;
        let range = (view_distance / CHUNK_SIZE).ceil() as i32 + 1;
        self.chunks.retain(|(cx, cz), chunk| {
            let d = ((chunk.center.x - rider.x).powi(2) + (chunk.center.z - rider.z).powi(2)).sqrt();
            let dx = (*cx - ccx).abs();
            let dz = (*cz - ccz).abs();
            d < view_distance + CHUNK_SIZE * 2.0 && dx <= range + 1 && dz <= range + 1
        });
        let mut candidates: Vec<(f32, i32, i32)> = Vec::new();
        for dz in -range..=range {
            for dx in -range..=range {
                let cx = ccx + dx;
                let cz = ccz + dz;
                let center_x = cx as f32 * CHUNK_SIZE + CHUNK_SIZE * 0.5;
                let center_z = cz as f32 * CHUNK_SIZE + CHUNK_SIZE * 0.5;
                let d = ((center_x - rider.x).powi(2) + (center_z - rider.z).powi(2)).sqrt();
                if d > view_distance + CHUNK_SIZE * 0.7 {
                    continue;
                }
                candidates.push((d, cx, cz));
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut built = 0;
        for (d, cx, cz) in candidates {
            if built >= budget {
                break;
            }
            let res = if d < 300.0 { 24 } else { 12 };
            let needs_build = match self.chunks.get(&(cx, cz)) {
                Some(chunk) => chunk.res != res || chunk.version != self.dem_version,
                None => true,
            };
            if !needs_build {
                continue;
            }
            let data = terrain.build_chunk(cx, cz, res);
            let (conifer, broadleaf) = split_trees(terrain.scatter_trees(
                cx,
                cz,
                self.quality.tree_density,
            ));
            let grass = if self.quality.grass {
                terrain.scatter_grass(cx, cz, self.quality.grass_density)
            } else {
                Vec::new()
            };
            match unsafe { GpuMesh::upload(gl, &data) } {
                Ok(mesh) => {
                    let center = v3(
                        cx as f32 * CHUNK_SIZE + CHUNK_SIZE * 0.5,
                        terrain.height(
                            cx as f32 * CHUNK_SIZE + CHUNK_SIZE * 0.5,
                            cz as f32 * CHUNK_SIZE + CHUNK_SIZE * 0.5,
                        ),
                        cz as f32 * CHUNK_SIZE + CHUNK_SIZE * 0.5,
                    );
                    self.chunks.insert(
                        (cx, cz),
                        Chunk {
                            mesh,
                            res,
                            version: self.dem_version,
                            center,
                            radius: CHUNK_SIZE * 0.78,
                            trees_conifer: conifer,
                            trees_broadleaf: broadleaf,
                            grass,
                        },
                    );
                    built += 1;
                }
                Err(err) => {
                    self.error = Some(err);
                    break;
                }
            }
        }
        self.terrain = Some(terrain);
    }

    fn ensure_derived(&mut self, gl: &glow::Context) {
        // Villages and lakes are expensive to derive; wait until the elevation
        // download has finished so they are built once from final terrain.
        if !self.dem_done {
            return;
        }
        if self.villages_version == self.dem_version && self.water.is_some() {
            return;
        }
        let Some(terrain) = self.terrain.take() else { return };
        let route = self.route.clone();
        if self.villages_version != self.dem_version {
            self.villages_version = self.dem_version;
            for (mesh, _, _) in self.villages.drain(..) {
                unsafe { mesh.destroy(gl) };
            }
            if let Some(route) = route {
                let villages: Vec<Village> = terrain.build_villages(&route);
                for village in villages {
                    if let Ok(gpu) = unsafe { GpuMesh::upload(gl, &village.mesh) } {
                        self.villages.push((gpu, village.center, village.radius));
                    }
                }
            }
        }
        if self.water.is_none() {
            if let Some(mesh) = terrain.build_water() {
                if let Ok(gpu) = unsafe { GpuMesh::upload(gl, &mesh) } {
                    self.water = Some(gpu);
                }
            }
        }
        self.terrain = Some(terrain);
    }

    fn update_camera(
        &mut self,
        dt: f32,
        rider: V3,
        forward: V3,
        station_forward: V3,
        terrain: &Terrain,
        prefs: &WorldPrefs,
    ) {
        update_camera(
            &mut self.camera,
            dt,
            rider,
            forward,
            station_forward,
            terrain,
            prefs,
        );
    }

    pub fn paint(
        &mut self,
        painter: &Painter,
        info: PaintCallbackInfo,
        input: &WorldInput,
        prefs: &WorldPrefs,
        stats: &mut WorldStats,
    ) {
        let gl = painter.gl();
        let now = Instant::now();
        let dt = self
            .last_paint
            .map(|last| now.duration_since(last).as_secs_f32().clamp(0.001, 0.2))
            .unwrap_or(1.0 / 60.0);
        self.last_paint = Some(now);
        self.elapsed += dt;
        self.frame_ema_ms = self.frame_ema_ms * 0.9 + dt * 1000.0 * 0.1;

        stats.gpu = self.gpu.clone();
        stats.initialized = self.programs.is_some();
        stats.frame_ms = self.frame_ema_ms;
        stats.fps = 1000.0 / self.frame_ema_ms.max(0.01);
        if let Some(err) = &self.error {
            stats.error = Some(err.clone());
            return;
        }

        let Some(route) = &input.route else {
            stats.error = Some("No route selected".to_string());
            return;
        };
        self.ensure_route(gl, route);
        self.poll_dem();
        self.quality = Quality::resolve(prefs.quality, super::GpuClass::classify(&self.gpu));

        let total = self
            .terrain
            .as_ref()
            .map(|t| t.path.total_m)
            .unwrap_or(1.0)
            .max(1.0);
        let target_distance = (input.distance_m as f32).clamp(0.0, total);
        let distance = if self.camera.initialized {
            let k = 1.0 - (-dt * 8.0).exp();
            self.smooth_distance += (target_distance - self.smooth_distance) * k;
            self.smooth_distance
        } else {
            self.smooth_distance = target_distance;
            target_distance
        };
        let (road_pos, station_forward, _heading, curvature) = {
            let Some(terrain) = &self.terrain else { return };
            let (pos, forward, heading) = terrain.path.frame_at(distance);
            (
                pos,
                forward,
                heading,
                terrain.path.station_at(distance).curvature,
            )
        };
        let right = v3(station_forward.z, 0.0, -station_forward.x);
        let rider = road_pos + right * 1.35 + V3::UP * 0.06;
        let speed = input.speed_mps.max(0.0);

        // Camera and animation.
        if let Some(terrain) = self.terrain.take() {
            self.update_camera(dt, rider, station_forward, station_forward, &terrain, prefs);
            self.terrain = Some(terrain);
        }

        let cadence = input.cadence_rpm.max(0.0);
        self.crank_angle = (self.crank_angle + cadence / 60.0 * std::f32::consts::TAU * dt)
            % std::f32::consts::TAU;
        self.wheel_angle = (self.wheel_angle + speed / 0.34 * dt) % std::f32::consts::TAU;
        let lean = (-(curvature * speed * speed / 9.81).atan()).clamp(-0.32, 0.32);

        let view = self.quality.view_distance;
        let budget = if self.initial_fill { 30 } else { 2 };
        self.update_chunks(gl, rider, view, budget);
        if self.initial_fill && !self.chunks.is_empty() {
            self.initial_fill = false;
        }
        self.ensure_derived(gl);
        stats.chunks = self.chunks.len();
        if let Some(terrain) = &self.terrain {
            stats.dem_tiles = terrain.dem.tile_count();
            stats.dem_status = terrain.dem.status;
            stats.terrain_credit = terrain.credit();
        }

        // Resolution scaling and target buffer.
        let vp = info.viewport_in_pixels();
        let base_scale = self
            .quality
            .scale
            .min(1.0)
            .max(0.45);
        self.adapt_counter += 1;
        if prefs.adaptive && self.adapt_counter >= 40 {
            self.adapt_counter = 0;
            if self.frame_ema_ms > 24.0 {
                self.adapt_factor = (self.adapt_factor - 0.07).max(0.5);
            } else if self.frame_ema_ms < 16.0 {
                self.adapt_factor = (self.adapt_factor + 0.04).min(1.0);
            }
        }
        let scale = (base_scale * self.adapt_factor).clamp(0.4, 1.0);
        stats.scale = scale;
        stats.quality = self.quality.preset_label();

        let pixel_width = ((vp.width_px as f32 * scale) as i32).max(64);
        let pixel_height = ((vp.height_px as f32 * scale) as i32).max(64);
        let aspect = pixel_width as f32 / pixel_height as f32;
        let fov = FOV_Y * if aspect < 1.4 { 1.08 } else { 1.0 };
        let view_mat = math::look_at(self.camera.eye, self.camera.target, V3::UP);
        let proj = math::perspective(fov, aspect, 0.25, view * 2.2);
        let viewproj = math::mul(proj, view_mat);
        let inv_viewproj = math::inverse(viewproj);
        let frustum = Frustum::from_view_projection(viewproj);

        let env = environment(prefs.hour, prefs.cloud, view, prefs.wet);
        stats.sun_elevation = env.sun_dir.y;

        // Recreate the offscreen target if the size changed.
        let needs_fbo = match &self.fbo {
            Some(fbo) => fbo.width != pixel_width || fbo.height != pixel_height,
            None => true,
        };
        if needs_fbo {
            if let Some(old) = self.fbo.take() {
                unsafe {
                    gl.delete_framebuffer(old.fbo);
                    gl.delete_texture(old.color);
                    gl.delete_renderbuffer(old.depth);
                }
            }
            match unsafe { SceneFbo::new(gl, pixel_width, pixel_height) } {
                Ok(fbo) => self.fbo = Some(fbo),
                Err(err) => {
                    stats.error = Some(err.clone());
                    self.error = Some(err);
                    return;
                }
            }
        }
        let Some(fbo) = &self.fbo else { return };
        let (Some(programs), Some(textures)) = (&self.programs, &self.textures) else {
            return;
        };

        // Animate the rider once; the mesh is reused by the shadow pass and the
        // main pass so the cast shadow always matches the visible pose.
        let avatar_mesh = {
            let look = geometry::RiderLook::default();
            geometry::build_cyclist(self.crank_angle, lean, self.wheel_angle, &look)
        };
        if let Some(avatar) = &mut self.avatar {
            unsafe { avatar.upload(gl, &avatar_mesh) };
        }
        let rider_heading = station_forward.x.atan2(station_forward.z);

        // Sun shadow map. Only the ground immediately around the rider is
        // rendered: at 1k² that is sharp where it matters and costs a fraction
        // of the main pass.
        let shadow_strength = if self.quality.shadows && env.sun_dir.y > 0.05 {
            0.85
        } else {
            0.0
        };
        let shadow_size = self.shadow_map.as_ref().map(|m| m.size).unwrap_or(1);
        let mut light_vp = math::IDENTITY;
        if shadow_strength > 0.0 {
            if let Some(map) = &self.shadow_map {
                let radius = self.quality.shadow_radius.max(20.0);
                let center = rider + station_forward * (radius * 0.3);
                light_vp = shadow_light_matrix(env.sun_dir, center, radius, map.size as f32);
                let mut cast_conifer: Vec<Instance> = Vec::new();
                let mut cast_broadleaf: Vec<Instance> = Vec::new();
                unsafe {
                    // The previous frame left the widget scissor enabled; it is
                    // in window pixels and would clip most of the shadow map.
                    gl.disable(glow::SCISSOR_TEST);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(map.fbo));
                    gl.viewport(0, 0, map.size, map.size);
                    gl.depth_mask(true);
                    gl.enable(glow::DEPTH_TEST);
                    gl.depth_func(glow::LEQUAL);
                    gl.enable(glow::CULL_FACE);
                    gl.cull_face(glow::BACK);
                    gl.clear(glow::DEPTH_BUFFER_BIT);
                    programs.depth.bind(gl);
                    programs.depth.mat4(gl, "u_light_viewproj", &light_vp);
                    programs.depth.mat4(gl, "u_model", &math::IDENTITY);
                    programs.depth.f32(gl, "u_instanced", 0.0);
                    for chunk in self.chunks.values() {
                        let d = (chunk.center - center).length();
                        if d < radius + CHUNK_SIZE {
                            chunk.mesh.draw(gl);
                        }
                        if d < radius + CHUNK_SIZE {
                            cast_conifer.extend_from_slice(&chunk.trees_conifer);
                            cast_broadleaf.extend_from_slice(&chunk.trees_broadleaf);
                        }
                    }
                    for (mesh, village_center, _) in &self.villages {
                        if (village_center.distance_xz(center)) < radius + 110.0 {
                            mesh.draw(gl);
                        }
                    }
                    if let Some(avatar) = &self.avatar {
                        let model = math::model_matrix(rider, rider_heading, 1.0);
                        programs.depth.mat4(gl, "u_model", &model);
                        avatar.gpu.draw(gl);
                        programs.depth.mat4(gl, "u_model", &math::IDENTITY);
                    }
                    programs.depth.f32(gl, "u_instanced", 1.0);
                    if let Some(conifer) = &self.conifer {
                        conifer.set_instances(gl, &cast_conifer);
                        conifer.draw(gl, cast_conifer.len());
                    }
                    if let Some(broadleaf) = &self.broadleaf {
                        broadleaf.set_instances(gl, &cast_broadleaf);
                        broadleaf.draw(gl, cast_broadleaf.len());
                    }
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo.fbo));
                    gl.viewport(0, 0, pixel_width, pixel_height);
                }
            }
        }

        let mut triangles = 0u32;
        let mut draws = 0u32;
        let mut visible_count = 0usize;
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo.fbo));
            gl.viewport(0, 0, pixel_width, pixel_height);
            gl.clear_color(env.fog_color.x, env.fog_color.y, env.fog_color.z, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::BLEND);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.enable(glow::CULL_FACE);
            gl.cull_face(glow::BACK);
            gl.front_face(glow::CCW);

            // Sky.
            gl.disable(glow::DEPTH_TEST);
            gl.depth_mask(false);
            programs.sky.bind(gl);
            programs.sky.mat4(gl, "u_inv_viewproj", &inv_viewproj);
            programs.sky.vec3(gl, "u_cam", self.camera.eye);
            programs.sky.vec3(gl, "u_sun_dir", env.sun_dir);
            programs.sky.vec3(gl, "u_sun_color", env.sun_color);
            programs.sky.vec3(gl, "u_zenith", env.zenith);
            programs.sky.vec3(gl, "u_horizon", env.horizon);
            programs.sky.vec3(gl, "u_ground", env.ground_color);
            programs.sky.f32(gl, "u_cloud", prefs.cloud);
            programs.sky.f32(gl, "u_time", self.elapsed);
            programs.sky.f32(gl, "u_night", env.night);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(textures.detail));
            gl.bind_vertex_array(self.empty_vao);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);

            // Terrain.
            programs.terrain.bind(gl);
            programs.terrain.mat4(gl, "u_viewproj", &viewproj);
            programs.terrain.vec3(gl, "u_cam", self.camera.eye);
            programs.terrain.vec3(gl, "u_sun_dir", env.sun_dir);
            programs.terrain.vec3(gl, "u_sun_color", env.sun_color);
            programs.terrain.vec3(gl, "u_sky_color", env.sky_color);
            programs.terrain.vec3(gl, "u_ground_color", env.ground_color);
            programs.terrain.vec3(gl, "u_fog_color", env.fog_color);
            programs.terrain.f32(gl, "u_fog_density", env.fog_density);
            set_shadow_uniforms(
                gl,
                &programs.terrain,
                &light_vp,
                shadow_strength,
                shadow_size,
                self.shadow_map.as_ref(),
            );
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(textures.grass));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(textures.rock));
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(textures.detail));
            let mut visible_chunks: Vec<(&(i32, i32), &Chunk)> = Vec::new();
            for (key, chunk) in &self.chunks {
                if !frustum.sphere_visible(chunk.center, chunk.radius) {
                    continue;
                }
                chunk.mesh.draw(gl);
                triangles += (chunk.mesh.count / 3) as u32;
                draws += 1;
                visible_count += 1;
                visible_chunks.push((key, chunk));
            }

            // Road.
            gl.disable(glow::CULL_FACE);
            programs.road.bind(gl);
            programs.road.mat4(gl, "u_viewproj", &viewproj);
            programs.road.vec3(gl, "u_cam", self.camera.eye);
            programs.road.vec3(gl, "u_sun_dir", env.sun_dir);
            programs.road.vec3(gl, "u_sun_color", env.sun_color);
            programs.road.vec3(gl, "u_sky_color", env.sky_color);
            programs.road.vec3(gl, "u_ground_color", env.ground_color);
            programs.road.vec3(gl, "u_fog_color", env.fog_color);
            programs.road.f32(gl, "u_fog_density", env.fog_density);
            programs.road.f32(gl, "u_wet", prefs.wet);
            set_shadow_uniforms(
                gl,
                &programs.road,
                &light_vp,
                shadow_strength,
                shadow_size,
                self.shadow_map.as_ref(),
            );
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(textures.asphalt));
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(textures.detail));
            if let Some((road, sections)) = &self.road {
                for section in sections {
                    let d = (section.distance_m - distance).abs();
                    if d > view * 1.2 {
                        continue;
                    }
                    road.draw_range(gl, section.first_index, section.count);
                    triangles += section.count / 3;
                    draws += 1;
                }
            }

            // Villages.
            gl.enable(glow::CULL_FACE);
            programs.mesh.bind(gl);
            set_mesh_uniforms(gl, &programs.mesh, &viewproj, &env, self.camera.eye, 0.0, 0.0);
            set_shadow_uniforms(
                gl,
                &programs.mesh,
                &light_vp,
                shadow_strength,
                shadow_size,
                self.shadow_map.as_ref(),
            );
            programs.mesh.f32(gl, "u_instanced", 0.0);
            programs.mesh.mat4(gl, "u_model", &math::IDENTITY);
            programs.mesh.f32(gl, "u_specular", 0.06);
            programs.mesh.f32(gl, "u_alpha_test", 0.0);
            for (mesh, center, radius) in &self.villages {
                if !frustum.sphere_visible(*center, *radius) {
                    continue;
                }
                mesh.draw(gl);
                triangles += (mesh.count / 3) as u32;
                draws += 1;
            }

            // Water.
            if let Some(water) = &self.water {
                if let Some(terrain) = &self.terrain {
                    if let Some(region) = terrain.water {
                        if frustum.sphere_visible(region.center, region.radius) {
                            gl.enable(glow::BLEND);
                            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                            gl.depth_mask(false);
                            gl.disable(glow::CULL_FACE);
                            programs.water.bind(gl);
                            programs.water.mat4(gl, "u_viewproj", &viewproj);
                            programs.water.vec3(gl, "u_cam", self.camera.eye);
                            programs.water.vec3(gl, "u_sun_dir", env.sun_dir);
                            programs.water.vec3(gl, "u_sun_color", env.sun_color);
                            programs.water.vec3(gl, "u_fog_color", env.fog_color);
                            programs.water.f32(gl, "u_fog_density", env.fog_density);
                            programs.water.f32(gl, "u_time", self.elapsed);
                            gl.active_texture(glow::TEXTURE0);
                            gl.bind_texture(glow::TEXTURE_2D, Some(textures.ripple));
                            water.draw(gl);
                            triangles += (water.count / 3) as u32;
                            draws += 1;
                            gl.depth_mask(true);
                            gl.disable(glow::BLEND);
                        }
                    }
                }
            }

            // Vegetation: gather visible instances, upload once, two draw calls.
            let tree_distance = self.quality.tree_distance.min(view);
            let (conifer, broadleaf, grass) = &mut self.instance_cache;
            conifer.clear();
            broadleaf.clear();
            grass.clear();
            for (_, chunk) in &visible_chunks {
                let d = ((chunk.center.x - rider.x).powi(2) + (chunk.center.z - rider.z).powi(2)).sqrt();
                if d < tree_distance {
                    conifer.extend_from_slice(&chunk.trees_conifer);
                    broadleaf.extend_from_slice(&chunk.trees_broadleaf);
                }
                if self.quality.grass && d < 90.0 {
                    grass.extend_from_slice(&chunk.grass);
                }
            }
            programs.mesh.bind(gl);
            programs.mesh.f32(gl, "u_instanced", 1.0);
            programs.mesh.f32(gl, "u_alpha_test", 0.0);
            programs.mesh.f32(gl, "u_specular", 0.02);
            programs.mesh.mat4(gl, "u_model", &math::IDENTITY);
            if let Some(conifer_mesh) = &self.conifer {
                conifer_mesh.set_instances(gl, conifer);
                conifer_mesh.draw(gl, conifer.len());
                triangles += (conifer_mesh.gpu.count / 3) as u32 * conifer.len() as u32;
                draws += 1;
            }
            if let Some(broadleaf_mesh) = &self.broadleaf {
                broadleaf_mesh.set_instances(gl, broadleaf);
                broadleaf_mesh.draw(gl, broadleaf.len());
                triangles += (broadleaf_mesh.gpu.count / 3) as u32 * broadleaf.len() as u32;
                draws += 1;
            }
            if self.quality.grass {
                if let Some(grass_mesh) = &self.grass {
                    gl.disable(glow::CULL_FACE);
                    programs.mesh.f32(gl, "u_alpha_test", 1.0);
                    programs.mesh.f32(gl, "u_specular", 0.0);
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(textures.alpha));
                    grass_mesh.set_instances(gl, grass);
                    grass_mesh.draw(gl, grass.len());
                    triangles += (grass_mesh.gpu.count / 3) as u32 * grass.len() as u32;
                    draws += 1;
                }
            }

            // Rider: contact shadow, then the animated cyclist.
            if let Some(shadow) = &self.shadow {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                gl.disable(glow::CULL_FACE);
                programs.shadow.bind(gl);
                programs.shadow.mat4(gl, "u_viewproj", &viewproj);
                let heading = station_forward.x.atan2(station_forward.z);
                let model = math::model_matrix(v3(rider.x, rider.y + 0.03, rider.z), heading, 1.0);
                programs.shadow.mat4(gl, "u_model", &model);
                programs.shadow.f32(gl, "u_strength", 0.42);
                shadow.draw(gl);
                draws += 1;
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
            }
            if let Some(avatar) = &mut self.avatar {
                programs.mesh.bind(gl);
                programs.mesh.f32(gl, "u_instanced", 0.0);
                programs.mesh.f32(gl, "u_alpha_test", 0.0);
                programs.mesh.f32(gl, "u_specular", 0.35);
                let model = math::model_matrix(rider, rider_heading, 1.0);
                programs.mesh.mat4(gl, "u_model", &model);
                gl.disable(glow::CULL_FACE);
                avatar.gpu.draw(gl);
                triangles += (avatar.gpu.count / 3) as u32;
                draws += 1;
            }

            // Post: FXAA-lite, ACES tonemap, vignette, then present.
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.enable(glow::SCISSOR_TEST);
            gl.viewport(
                vp.left_px,
                vp.from_bottom_px,
                vp.width_px,
                vp.height_px,
            );
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            programs.post.bind(gl);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(fbo.color));
            gl.uniform_2_f32(
                programs.post.loc("u_texel"),
                1.0 / pixel_width as f32,
                1.0 / pixel_height as f32,
            );
            programs.post.f32(gl, "u_exposure", env.exposure);
            programs.post.f32(gl, "u_fxaa", if self.quality.fxaa { 1.0 } else { 0.0 });
            programs.post.f32(gl, "u_vignette", 0.22);
            gl.bind_vertex_array(self.empty_vao);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }

        stats.triangles = triangles;
        stats.draws = draws;
        stats.visible_chunks = visible_count;
        stats.error = None;
    }
}

/// Orthographic sun matrix for the shadow map, snapped to whole texels in
/// light space so edges do not crawl while riding.
fn shadow_light_matrix(sun: V3, center: V3, radius: f32, texels: f32) -> [f32; 16] {
    let up = if sun.y.abs() > 0.97 {
        v3(1.0, 0.0, 0.0)
    } else {
        V3::UP
    };
    let eye = center + sun * (radius * 2.4);
    let view = math::look_at(eye, center, up);
    let proj = math::ortho(-radius, radius, -radius, radius, 0.5, radius * 5.0);
    let vp = math::mul(proj, view);
    let texel = 2.0 * radius / texels.max(1.0);
    let light = math::mul_vec4(vp, [center.x, center.y, center.z, 1.0]);
    let inv = math::inverse(vp);
    let snapped = math::mul_vec4(
        inv,
        [
            (light[0] / light[3] / texel).round() * texel * light[3],
            (light[1] / light[3] / texel).round() * texel * light[3],
            light[2],
            light[3],
        ],
    );
    if snapped[3].abs() < 1e-6 {
        return vp;
    }
    let center = v3(
        snapped[0] / snapped[3],
        snapped[1] / snapped[3],
        snapped[2] / snapped[3],
    );
    let eye = center + sun * (radius * 2.4);
    math::mul(
        math::ortho(-radius, radius, -radius, radius, 0.5, radius * 5.0),
        math::look_at(eye, center, up),
    )
}

unsafe fn set_shadow_uniforms(
    gl: &glow::Context,
    program: &GlProgram,
    light_viewproj: &[f32; 16],
    strength: f32,
    size: i32,
    map: Option<&ShadowMap>,
) {
    program.mat4(gl, "u_light_viewproj", light_viewproj);
    program.f32(gl, "u_shadow_strength", strength);
    let texel = 1.0 / size.max(1) as f32;
    program.vec2(gl, "u_shadow_texel", texel, texel);
    if let Some(map) = map {
        gl.active_texture(glow::TEXTURE3);
        gl.bind_texture(glow::TEXTURE_2D, Some(map.depth));
    }
}

/// Smooth, terrain-aware third-person cycling camera.
fn update_camera(
    cam: &mut CameraState,
    dt: f32,
    rider: V3,
    forward: V3,
    station_forward: V3,
    terrain: &Terrain,
    prefs: &WorldPrefs,
) {
    let (base_distance, base_pitch, base_height) = match prefs.camera.mode {
        CameraMode::Chase => (7.2, 0.17, 0.95),
        CameraMode::Close => (4.1, 0.13, 0.85),
        CameraMode::Hood => (1.9, 0.04, 1.25),
        CameraMode::Drone => (15.0, 0.52, 1.2),
    };
    let distance = base_distance * cam.distance_scale;
    let yaw = station_forward.x.atan2(station_forward.z) + cam.yaw_offset;
    let f = v3(yaw.sin(), 0.0, yaw.cos());
    let pitch = (base_pitch + cam.pitch_offset).clamp(-0.25, 1.3);
    let horizontal = distance * pitch.cos();
    let mut eye = rider - f * horizontal + V3::UP * (base_height + distance * pitch.sin());
    if prefs.camera.mode == CameraMode::Hood {
        eye = rider + f * 0.2 + V3::UP * 1.35;
    }
    let ground = terrain.height(eye.x, eye.z) + 1.1;
    if eye.y < ground {
        eye.y = ground;
    }
    let mut target = rider + forward * 2.4 + V3::UP * 1.05;
    if prefs.camera.mode == CameraMode::Hood {
        target = rider + forward * 9.0 + V3::UP * 0.9;
    }
    if !cam.initialized {
        cam.eye = eye;
        cam.target = target;
        cam.initialized = true;
    } else {
        let k_eye = 1.0 - (-dt * 6.5).exp();
        let k_target = 1.0 - (-dt * 9.0).exp();
        cam.eye = cam.eye.lerp(eye, k_eye);
        cam.target = cam.target.lerp(target, k_target);
    }
}

unsafe fn set_mesh_uniforms(
    gl: &glow::Context,
    program: &GlProgram,
    viewproj: &[f32; 16],
    env: &Environment,
    camera: V3,
    specular: f32,
    alpha_test: f32,
) {
    program.mat4(gl, "u_viewproj", viewproj);
    program.vec3(gl, "u_cam", camera);
    program.vec3(gl, "u_sun_dir", env.sun_dir);
    program.vec3(gl, "u_sun_color", env.sun_color);
    program.vec3(gl, "u_sky_color", env.sky_color);
    program.vec3(gl, "u_ground_color", env.ground_color);
    program.vec3(gl, "u_fog_color", env.fog_color);
    program.f32(gl, "u_fog_density", env.fog_density);
    program.f32(gl, "u_specular", specular);
    program.f32(gl, "u_alpha_test", alpha_test);
}

fn split_trees(trees: Vec<Instance>) -> (Vec<Instance>, Vec<Instance>) {
    let mut conifer = Vec::with_capacity(trees.len() / 2);
    let mut broadleaf = Vec::with_capacity(trees.len() / 2);
    for tree in trees {
        if tree.rot_tint[2] < 0.5 {
            conifer.push(tree);
        } else {
            broadleaf.push(tree);
        }
    }
    (conifer, broadleaf)
}

unsafe fn upload_texture(
    gl: &glow::Context,
    width: u32,
    height: u32,
    data: &[u8],
    srgb: bool,
) -> Result<glow::Texture, String> {
    let texture = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::LINEAR_MIPMAP_LINEAR as i32,
    );
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    // Colour maps must be decoded from sRGB before lighting, otherwise every
    // albedo is ~2x too bright and the whole world looks washed out.
    let internal = if srgb {
        glow::SRGB8_ALPHA8 as i32
    } else {
        glow::RGBA8 as i32
    };
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        internal,
        width as i32,
        height as i32,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(data)),
    );
    gl.generate_mipmap(glow::TEXTURE_2D);
    Ok(texture)
}

// ---------------------------------------------------------------------------
// Procedural textures. They are tiny (64–128 px), generated once and sampled
// with mipmaps, so VRAM cost stays in the low single-digit megabytes.
// ---------------------------------------------------------------------------

fn grass_texture(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            let clumps = super::terrain::value_noise(fx * 0.06, fy * 0.06, 11);
            let blades = super::terrain::value_noise(fx * 0.55, fy * 0.12, 23);
            let fine = super::terrain::value_noise(fx * 1.7, fy * 1.7, 41);
            let v = 0.42 + clumps * 0.14 + blades * 0.18 + fine * 0.12;
            let r = (38.0 + v * 42.0).clamp(0.0, 255.0) as u8;
            let g = (58.0 + v * 78.0).clamp(0.0, 255.0) as u8;
            let b = (24.0 + v * 30.0).clamp(0.0, 255.0) as u8;
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    data
}

fn rock_texture(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            let strata = super::terrain::fbm(fx * 0.04, fy * 0.012, 4, 7);
            let grain = super::terrain::value_noise(fx * 0.9, fy * 0.9, 13);
            let crack = 1.0 - (super::terrain::value_noise(fx * 0.08, fy * 0.06, 31).abs() * 3.0).min(1.0);
            let v = 0.5 + strata * 0.22 + grain * 0.12 - crack * 0.12;
            let r = (90.0 + v * 100.0).clamp(0.0, 255.0) as u8;
            let g = (86.0 + v * 94.0).clamp(0.0, 255.0) as u8;
            let b = (80.0 + v * 88.0).clamp(0.0, 255.0) as u8;
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    data
}

fn asphalt_texture(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            let speckle = super::terrain::value_noise(fx * 1.6, fy * 1.6, 5);
            let patches = super::terrain::fbm(fx * 0.05, fy * 0.05, 3, 9);
            let v = 0.4 + speckle * 0.12 + patches * 0.08;
            let r = (70.0 + v * 60.0).clamp(0.0, 255.0) as u8;
            let g = (72.0 + v * 60.0).clamp(0.0, 255.0) as u8;
            let b = (74.0 + v * 58.0).clamp(0.0, 255.0) as u8;
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    data
}

fn normal_texture(size: usize, seed_offset: f32) -> Vec<u8> {
    let mut heights = vec![0.0f32; size * size];
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 + seed_offset;
            let fy = y as f32 + seed_offset;
            heights[y * size + x] = super::terrain::fbm(fx * 0.045, fy * 0.045, 4, 3)
                + 0.45 * super::terrain::value_noise(fx * 0.22, fy * 0.22, 17);
        }
    }
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let l = heights[y * size + (x + size - 1) % size];
            let r = heights[y * size + (x + 1) % size];
            let u = heights[(y + size - 1) % size * size + x];
            let d = heights[(y + 1) % size * size + x];
            let nx = (l - r) * 1.6;
            let ny = (u - d) * 1.6;
            let nz = 1.0f32;
            let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-4);
            let px = ((nx / len) * 0.5 + 0.5) * 255.0;
            let py = ((ny / len) * 0.5 + 0.5) * 255.0;
            let pz = ((nz / len) * 0.5 + 0.5) * 255.0;
            let alpha = ((heights[y * size + x] * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            data.extend_from_slice(&[px as u8, py as u8, pz as u8, alpha]);
        }
    }
    data
}

fn blade_texture(size: usize) -> Vec<u8> {
    let mut data = vec![0u8; size * size * 4];
    for blade in 0..14 {
        let h = math::hash_u32(blade as u32 * 97 + 3);
        let cx = (h & 0xff) as f32 / 255.0 * size as f32;
        let width = 1.2 + ((h >> 8) & 0xff) as f32 / 255.0 * 2.4;
        let height = size as f32 * (0.55 + ((h >> 16) & 0xff) as f32 / 255.0 * 0.45);
        let lean = (((h >> 24) & 0xff) as f32 / 255.0 - 0.5) * 10.0;
        for y in 0..height as usize {
            let t = y as f32 / height;
            let center = cx + lean * t * t;
            let w = width * (1.0 - t * 0.75);
            for x in 0..size {
                let dx = (x as f32 - center).abs();
                if dx <= w {
                    let alpha = if dx > w - 0.8 { 140 } else { 255 };
                    let idx = ((size - 1 - y) * size + x) * 4;
                    data[idx] = 210;
                    data[idx + 1] = 230;
                    data[idx + 2] = 190;
                    data[idx + 3] = alpha;
                }
            }
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_have_expected_size() {
        assert_eq!(grass_texture(32).len(), 32 * 32 * 4);
        assert_eq!(normal_texture(16, 0.0).len(), 16 * 16 * 4);
        let blades = blade_texture(32);
        assert!(blades.chunks_exact(4).any(|p| p[3] > 0));
        assert!(blades.chunks_exact(4).any(|p| p[3] == 0));
    }

    #[test]
    fn sun_moves_through_the_day() {
        let morning = environment(8.0, 0.0, 600.0, 0.0);
        let noon = environment(12.0, 0.0, 600.0, 0.0);
        let night = environment(23.0, 0.0, 600.0, 0.0);
        assert!(noon.sun_dir.y > morning.sun_dir.y);
        assert!(night.sun_dir.y < 0.0);
        assert!(night.night > 0.5);
    }
}
