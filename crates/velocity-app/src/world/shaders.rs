//! GLSL 330 shaders for the virtual world.
//!
//! The look targets "premium virtual cycling game" on integrated graphics:
//! physically based-ish lighting (one sun + hemisphere ambient), procedural
//! albedo detail sampled from small tiling textures, exponential distance fog
//! that matches the sky, and a single post pass (FXAA-lite + ACES tonemapping).

pub const VS_TERRAIN: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;
layout(location = 3) in vec3 a_color;

uniform mat4 u_viewproj;

out vec3 v_world;
out vec3 v_normal;
out vec2 v_uv;
out vec3 v_tint;

void main() {
    v_world = a_pos;
    v_normal = a_normal;
    v_uv = a_uv;
    v_tint = a_color;
    gl_Position = u_viewproj * vec4(a_pos, 1.0);
}
"#;

pub const FS_TERRAIN: &str = r#"#version 330 core
in vec3 v_world;
in vec3 v_normal;
in vec2 v_uv;
in vec3 v_tint;

uniform vec3 u_cam;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_color;
uniform vec3 u_sky_color;
uniform vec3 u_ground_color;
uniform vec3 u_fog_color;
uniform float u_fog_density;
uniform sampler2D u_grass;
uniform sampler2D u_rock;
uniform sampler2D u_detail;
uniform sampler2D u_shadow_map;
uniform mat4 u_light_viewproj;
uniform float u_shadow_strength;
uniform vec2 u_shadow_texel;

out vec4 out_color;

float sun_visibility(vec3 world, vec3 normal) {
    if (u_shadow_strength < 0.01) {
        return 1.0;
    }
    vec4 light = u_light_viewproj * vec4(world + normal * 0.14, 1.0);
    vec3 p = light.xyz / light.w * 0.5 + 0.5;
    if (p.x <= 0.002 || p.x >= 0.998 || p.y <= 0.002 || p.y >= 0.998 || p.z >= 1.0) {
        return 1.0;
    }
    float bias = 0.0016 + 0.0032 * (1.0 - clamp(normal.y, 0.0, 1.0));
    float lit = 0.0;
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2(-0.7, -0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2( 0.7, -0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2(-0.7,  0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2( 0.7,  0.7) * u_shadow_texel).r);
    return mix(1.0, lit * 0.25, u_shadow_strength);
}

void main() {
    vec3 N = normalize(v_normal);
    float slope = 1.0 - clamp(N.y, 0.0, 1.0);

    vec3 grass_tex = texture(u_grass, v_world.xz * 0.035).rgb;
    vec3 rock_tex = texture(u_rock, v_world.xz * 0.021).rgb;
    float patch = texture(u_grass, v_world.xz * 0.0009).g;
    float broad = texture(u_rock, v_world.xz * 0.0025).r;

    vec3 meadow = mix(vec3(0.18, 0.27, 0.09), vec3(0.33, 0.39, 0.15), patch);
    meadow *= 0.75 + grass_tex * 1.8;
    vec3 stone = rock_tex * vec3(0.95, 0.92, 0.88) * 1.7;
    float rock_mask = smoothstep(0.26, 0.55, slope + broad * 0.28);
    vec3 albedo = mix(meadow, stone, rock_mask);

    // Alpine grass and snow-free scree at altitude.
    float altitude = smoothstep(1150.0, 1750.0, v_world.y);
    vec3 high = mix(stone * 0.92, vec3(0.66, 0.67, 0.7), 0.45);
    albedo = mix(albedo, high, altitude * 0.55);
    albedo *= v_tint;

    // Surface micro-relief: this is what keeps 5 m geometry from looking flat.
    vec3 dn = texture(u_detail, v_world.xz * 0.24).rgb * 2.0 - 1.0;
    N = normalize(N + vec3(dn.x, 0.0, dn.z) * (0.5 - rock_mask * 0.2));

    float ndl = max(dot(N, u_sun_dir), 0.0) * sun_visibility(v_world, N);
    vec3 hemi = mix(u_ground_color, u_sky_color, N.y * 0.5 + 0.5);
    vec3 lit = albedo * (u_sun_color * ndl + hemi);

    // Distance fog matched to the sky's horizon colour.
    float dist = length(v_world - u_cam);
    float fog = 1.0 - exp(-pow(dist * u_fog_density, 2.0));
    vec3 col = mix(lit, u_fog_color, clamp(fog, 0.0, 1.0));
    out_color = vec4(col, 1.0);
}
"#;

pub const VS_ROAD: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;
layout(location = 3) in vec3 a_color;

uniform mat4 u_viewproj;

out vec3 v_world;
out vec3 v_normal;
out vec2 v_uv;
out vec3 v_tint;

void main() {
    v_world = a_pos;
    v_normal = a_normal;
    v_uv = a_uv;
    v_tint = a_color;
    gl_Position = u_viewproj * vec4(a_pos, 1.0);
}
"#;

pub const FS_ROAD: &str = r#"#version 330 core
in vec3 v_world;
in vec3 v_normal;
in vec2 v_uv;
in vec3 v_tint;

uniform vec3 u_cam;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_color;
uniform vec3 u_sky_color;
uniform vec3 u_ground_color;
uniform vec3 u_fog_color;
uniform float u_fog_density;
uniform float u_wet;
uniform sampler2D u_asphalt;
uniform sampler2D u_detail;
uniform sampler2D u_shadow_map;
uniform mat4 u_light_viewproj;
uniform float u_shadow_strength;
uniform vec2 u_shadow_texel;

out vec4 out_color;

float sun_visibility(vec3 world, vec3 normal) {
    if (u_shadow_strength < 0.01) {
        return 1.0;
    }
    vec4 light = u_light_viewproj * vec4(world + normal * 0.1, 1.0);
    vec3 p = light.xyz / light.w * 0.5 + 0.5;
    if (p.x <= 0.002 || p.x >= 0.998 || p.y <= 0.002 || p.y >= 0.998 || p.z >= 1.0) {
        return 1.0;
    }
    float bias = 0.0016;
    float lit = 0.0;
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2(-0.7, -0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2( 0.7, -0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2(-0.7,  0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2( 0.7,  0.7) * u_shadow_texel).r);
    return mix(1.0, lit * 0.25, u_shadow_strength);
}

void main() {
    float lat = v_uv.x;
    float along = v_uv.y;

    vec3 asphalt = texture(u_asphalt, vec2(along * 0.09, lat * 1.6)).rgb;
    vec3 gravel = texture(u_asphalt, vec2(along * 0.3, lat * 7.0)).rgb * vec3(1.05, 0.95, 0.78);
    float shoulder = smoothstep(1.0, 1.12, abs(lat));
    vec3 albedo = mix(asphalt * 0.42, gravel * 0.72, shoulder);

    float grain = texture(u_detail, v_world.xz * 0.75).r;
    albedo *= 0.82 + grain * 0.33;

    // Paint: solid edge lines and a dashed centre line, in lane space.
    float edge = 1.0 - smoothstep(0.035, 0.06, abs(abs(lat) - 0.9));
    float dash = step(abs(lat), 0.055) * step(fract(along / 9.0), 0.34);
    float paint = clamp(edge + dash, 0.0, 1.0) * (1.0 - shoulder);
    vec3 paint_color = vec3(0.78, 0.78, 0.72) * v_tint;
    albedo = mix(albedo, paint_color, paint * 0.85);

    vec3 N = normalize(v_normal);
    vec3 dn = texture(u_detail, v_world.xz * 0.55).rgb * 2.0 - 1.0;
    N = normalize(N + vec3(dn.x, 0.0, dn.z) * 0.12);

    float shadow = sun_visibility(v_world, N);
    float ndl = max(dot(N, u_sun_dir), 0.0) * shadow;
    vec3 hemi = mix(u_ground_color, u_sky_color, N.y * 0.5 + 0.5);
    vec3 V = normalize(u_cam - v_world);
    vec3 H = normalize(u_sun_dir + V);
    float spec_power = mix(48.0, 260.0, u_wet);
    float spec = pow(max(dot(N, H), 0.0), spec_power) * mix(0.05, 0.55, u_wet) * shadow;
    vec3 lit = albedo * (u_sun_color * ndl + hemi) + u_sun_color * spec;

    float dist = length(v_world - u_cam);
    float fog = 1.0 - exp(-pow(dist * u_fog_density, 2.0));
    vec3 col = mix(lit, u_fog_color, clamp(fog, 0.0, 1.0));
    out_color = vec4(col, 1.0);
}
"#;

pub const VS_MESH: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;
layout(location = 3) in vec3 a_color;

layout(location = 4) in vec4 a_instance_pos_scale;
layout(location = 5) in vec4 a_instance_rot_tint;

uniform mat4 u_viewproj;
uniform mat4 u_model;
uniform float u_instanced;

out vec3 v_world;
out vec3 v_normal;
out vec2 v_uv;
out vec3 v_color;

void main() {
    vec3 pos = a_pos;
    vec3 normal = a_normal;
    if (u_instanced > 0.5) {
        float s = a_instance_pos_scale.w;
        float yaw = a_instance_rot_tint.x;
        float c = cos(yaw);
        float si = sin(yaw);
        pos = vec3(pos.x * c + pos.z * si, pos.y, -pos.x * si + pos.z * c) * s
            + a_instance_pos_scale.xyz;
        normal = vec3(normal.x * c + normal.z * si, normal.y, -normal.x * si + normal.z * c);
    }
    vec4 world = u_model * vec4(pos, 1.0);
    v_world = world.xyz;
    v_normal = normal;
    v_uv = a_uv;
    // Per-instance tint keeps trees and grass from looking cloned.
    v_color = a_color * mix(vec3(1.0), vec3(0.82 + a_instance_rot_tint.y * 0.42), u_instanced);
    gl_Position = u_viewproj * world;
}
"#;

pub const FS_MESH: &str = r#"#version 330 core
in vec3 v_world;
in vec3 v_normal;
in vec2 v_uv;
in vec3 v_color;

uniform vec3 u_cam;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_color;
uniform vec3 u_sky_color;
uniform vec3 u_ground_color;
uniform vec3 u_fog_color;
uniform float u_fog_density;
uniform float u_specular;
uniform float u_alpha_test;
uniform sampler2D u_alpha;
uniform sampler2D u_shadow_map;
uniform mat4 u_light_viewproj;
uniform float u_shadow_strength;
uniform vec2 u_shadow_texel;

out vec4 out_color;

float sun_visibility(vec3 world, vec3 normal) {
    if (u_shadow_strength < 0.01) {
        return 1.0;
    }
    vec4 light = u_light_viewproj * vec4(world + normal * 0.14, 1.0);
    vec3 p = light.xyz / light.w * 0.5 + 0.5;
    if (p.x <= 0.002 || p.x >= 0.998 || p.y <= 0.002 || p.y >= 0.998 || p.z >= 1.0) {
        return 1.0;
    }
    float bias = 0.0018 + 0.0035 * (1.0 - clamp(normal.y, 0.0, 1.0));
    float lit = 0.0;
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2(-0.7, -0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2( 0.7, -0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2(-0.7,  0.7) * u_shadow_texel).r);
    lit += step(p.z - bias, texture(u_shadow_map, p.xy + vec2( 0.7,  0.7) * u_shadow_texel).r);
    return mix(1.0, lit * 0.25, u_shadow_strength);
}

void main() {
    if (u_alpha_test > 0.5) {
        if (texture(u_alpha, v_uv).a < 0.45) {
            discard;
        }
    }
    vec3 albedo = v_color;
    vec3 N = normalize(v_normal);
    float shadow = sun_visibility(v_world, N);
    float ndl = max(dot(N, u_sun_dir), 0.0) * shadow;
    vec3 hemi = mix(u_ground_color, u_sky_color, N.y * 0.5 + 0.5);
    vec3 lit = albedo * (u_sun_color * ndl + hemi);
    vec3 V = normalize(u_cam - v_world);
    vec3 H = normalize(u_sun_dir + V);
    lit += u_sun_color * pow(max(dot(N, H), 0.0), 36.0) * u_specular * shadow;

    float dist = length(v_world - u_cam);
    float fog = 1.0 - exp(-pow(dist * u_fog_density, 2.0));
    vec3 col = mix(lit, u_fog_color, clamp(fog, 0.0, 1.0));
    out_color = vec4(col, 1.0);
}
"#;

pub const VS_WATER: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;
layout(location = 3) in vec3 a_color;

uniform mat4 u_viewproj;

out vec3 v_world;
out vec2 v_uv;

void main() {
    v_world = a_pos;
    v_uv = a_uv;
    gl_Position = u_viewproj * vec4(a_pos, 1.0);
}
"#;

pub const FS_WATER: &str = r#"#version 330 core
in vec3 v_world;
in vec2 v_uv;

uniform vec3 u_cam;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_color;
uniform vec3 u_fog_color;
uniform float u_fog_density;
uniform float u_time;
uniform sampler2D u_ripple;

out vec4 out_color;

void main() {
    vec3 V = normalize(u_cam - v_world);
    vec2 uv = v_world.xz * 0.035;
    vec3 n1 = texture(u_ripple, uv + vec2(u_time * 0.008, u_time * 0.005)).rgb * 2.0 - 1.0;
    vec3 n2 = texture(u_ripple, uv * 2.7 - vec2(u_time * 0.011, 0.0)).rgb * 2.0 - 1.0;
    vec3 N = normalize(vec3((n1.x + n2.x * 0.6) * 0.28, 1.0, (n1.z + n2.z * 0.6) * 0.28));

    float fresnel = 0.03 + 0.97 * pow(1.0 - max(dot(N, V), 0.0), 5.0);
    vec3 deep = vec3(0.015, 0.055, 0.075);
    vec3 reflection = mix(u_fog_color, vec3(0.5, 0.65, 0.82), 0.35);
    vec3 col = mix(deep, reflection, clamp(fresnel, 0.0, 0.9));

    vec3 H = normalize(u_sun_dir + V);
    col += u_sun_color * pow(max(dot(N, H), 0.0), 340.0) * 2.2;

    float dist = length(v_world - u_cam);
    float fog = 1.0 - exp(-pow(dist * u_fog_density, 2.0));
    col = mix(col, u_fog_color, clamp(fog, 0.0, 1.0));
    out_color = vec4(col, 0.9);
}
"#;

pub const VS_SKY: &str = r#"#version 330 core
out vec2 v_ndc;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_ndc = p * 2.0 - 1.0;
    gl_Position = vec4(v_ndc, 1.0, 1.0);
}
"#;

pub const FS_SKY: &str = r#"#version 330 core
in vec2 v_ndc;

uniform mat4 u_inv_viewproj;
uniform vec3 u_cam;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_color;
uniform vec3 u_zenith;
uniform vec3 u_horizon;
uniform vec3 u_ground;
uniform float u_cloud;
uniform float u_time;
uniform float u_night;
uniform sampler2D u_detail;

out vec4 out_color;

void main() {
    vec4 p = u_inv_viewproj * vec4(v_ndc, 1.0, 1.0);
    vec3 dir = normalize(p.xyz / p.w - u_cam);

    float up = clamp(dir.y, -1.0, 1.0);
    float horizon_band = pow(1.0 - clamp(abs(up), 0.0, 1.0), 3.0);
    vec3 sky = mix(u_zenith, u_horizon, horizon_band * 0.85);
    if (up < 0.0) {
        sky = mix(u_horizon, u_ground * 0.55, clamp(-up * 3.0, 0.0, 1.0));
    }

    // Sun disc and broad forward-scattering halo.
    float sun_dot = max(dot(dir, u_sun_dir), 0.0);
    sky += u_sun_color * (pow(sun_dot, 2200.0) * 14.0 + pow(sun_dot, 22.0) * 0.14);

    // Clouds projected onto a high plane, still cheap enough for HD Graphics.
    if (up > 0.015) {
        float t = 1500.0 / max(up, 0.06);
        vec2 cp = (u_cam.xz + dir.xz * t) * 0.00042 + vec2(u_time * 0.0011, u_time * 0.0007);
        float n = texture(u_detail, cp).r * 0.55
                + texture(u_detail, cp * 2.3 + 0.31).r * 0.28
                + texture(u_detail, cp * 5.4 + 0.77).r * 0.17;
        float coverage = smoothstep(1.02 - u_cloud * 0.9, 1.22 - u_cloud * 0.9, n * 1.08);
        float fade = smoothstep(0.015, 0.22, up);
        vec3 lit_cloud = mix(vec3(0.36, 0.4, 0.46), vec3(0.95, 0.95, 0.96), 0.55 + 0.45 * sun_dot);
        sky = mix(sky, lit_cloud * (1.0 - u_night * 0.8), coverage * fade * clamp(u_cloud, 0.0, 1.0) * 0.85);
    }

    // A handful of stars, only at night and only above the horizon.
    if (u_night > 0.4 && up > 0.05) {
        vec3 s = floor(dir * 260.0);
        float star = step(0.9985, fract(sin(dot(s, vec3(12.9898, 78.233, 45.164))) * 43758.5453));
        sky += vec3(0.8, 0.85, 1.0) * star * (u_night - 0.4) * 1.4;
    }

    out_color = vec4(sky, 1.0);
}
"#;

pub const VS_SHADOW: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 2) in vec2 a_uv;
uniform mat4 u_viewproj;
uniform mat4 u_model;
out vec2 v_uv;
void main() {
    v_uv = a_uv;
    gl_Position = u_viewproj * u_model * vec4(a_pos, 1.0);
}
"#;

pub const FS_SHADOW: &str = r#"#version 330 core
in vec2 v_uv;
uniform float u_strength;
out vec4 out_color;
void main() {
    float d = length(v_uv * 2.0 - 1.0);
    float a = (1.0 - smoothstep(0.35, 1.0, d)) * u_strength;
    out_color = vec4(0.0, 0.0, 0.0, a);
}
"#;

/// Depth-only pass used by the sun shadow map. Instanced trees share it.
pub const VS_SHADOW_DEPTH: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 4) in vec4 a_instance_pos_scale;
layout(location = 5) in vec4 a_instance_rot_tint;
uniform mat4 u_light_viewproj;
uniform mat4 u_model;
uniform float u_instanced;
void main() {
    vec3 p = a_pos;
    if (u_instanced > 0.5) {
        float s = a_instance_pos_scale.w;
        float yaw = a_instance_rot_tint.x;
        float c = cos(yaw);
        float si = sin(yaw);
        p = vec3(p.x * c + p.z * si, p.y, -p.x * si + p.z * c) * s + a_instance_pos_scale.xyz;
    }
    gl_Position = u_light_viewproj * u_model * vec4(p, 1.0);
}
"#;

pub const FS_SHADOW_DEPTH: &str = r#"#version 330 core
void main() {}
"#;

pub const VS_POST: &str = r#"#version 330 core
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

pub const FS_POST: &str = r#"#version 330 core
in vec2 v_uv;

uniform sampler2D u_scene;
uniform vec2 u_texel;
uniform float u_exposure;
uniform float u_fxaa;
uniform float u_vignette;

out vec4 out_color;

float luma(vec3 c) {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

void main() {
    vec3 col = texture(u_scene, v_uv).rgb;
    if (u_fxaa > 0.5) {
        vec3 n = texture(u_scene, v_uv + vec2(0.0, u_texel.y)).rgb;
        vec3 s = texture(u_scene, v_uv - vec2(0.0, u_texel.y)).rgb;
        vec3 e = texture(u_scene, v_uv + vec2(u_texel.x, 0.0)).rgb;
        vec3 w = texture(u_scene, v_uv - vec2(u_texel.x, 0.0)).rgb;
        float lc = luma(col);
        float contrast = max(max(abs(luma(n) - lc), abs(luma(s) - lc)),
                             max(abs(luma(e) - lc), abs(luma(w) - lc)));
        if (contrast > 0.12) {
            col = (col * 2.0 + n + s + e + w) / 6.0;
        }
    }

    col = aces(col * u_exposure);
    col = pow(col, vec3(1.0 / 2.2));
    vec2 d = v_uv - 0.5;
    col *= 1.0 - u_vignette * dot(d, d);
    out_color = vec4(col, 1.0);
}
"#;
