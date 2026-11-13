struct SceneUniform {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    camera_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_sky: vec4<f32>,
    ambient_ground: vec4<f32>,
    sky_zenith: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniform;

@group(0) @binding(1)
var t_atlas: texture_2d_array<f32>;

@group(0) @binding(2)
var s_atlas: sampler;

@group(1) @binding(0)
var t_shadow: texture_depth_2d;

@group(1) @binding(1)
var s_shadow: sampler_comparison;

// Filmic Reinhard Tone Mapping (adapted for linear-to-sRGB hardware pipelines)
fn tonemap_filmic(color: vec3<f32>) -> vec3<f32> {
    let x = max(color, vec3<f32>(0.0));
    return x / (vec3<f32>(1.0) + x * 0.85);
}

fn hash2_clouds_grid(ix: i32, iy: i32) -> f32 {
    let ux = bitcast<u32>(ix);
    let uy = bitcast<u32>(iy);
    var n = ux * 1597334673u ^ uy * 3812015801u;
    n = n * (n ^ (n >> 15u));
    n = n * (n ^ (n >> 15u));
    return f32(n ^ (n >> 16u)) * (1.0 / 4294967295.0);
}

fn noise2_clouds(p: vec2<f32>) -> f32 {
    let fl = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let ix = i32(fl.x);
    let iy = i32(fl.y);
    let a = hash2_clouds_grid(ix, iy);
    let b = hash2_clouds_grid(ix + 1, iy);
    let c = hash2_clouds_grid(ix, iy + 1);
    let d = hash2_clouds_grid(ix + 1, iy + 1);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm_clouds(p: vec2<f32>) -> f32 {
    var v = noise2_clouds(p) * 0.52;
    v += noise2_clouds(p * 2.03 + vec2<f32>(13.2, 7.8)) * 0.28;
    v += noise2_clouds(p * 4.07 + vec2<f32>(31.4, 19.1)) * 0.14;
    v += noise2_clouds(p * 8.13 + vec2<f32>(53.7, 41.3)) * 0.06;
    return v;
}

fn calculate_shadow(world_pos: vec3<f32>, normal: vec3<f32>) -> f32 {
    let bias_pos = world_pos + normal * 0.04;
    let light_clip = scene.light_view_proj * vec4<f32>(bias_pos, 1.0);
    let shadow_ndc = light_clip.xyz / light_clip.w;

    if (shadow_ndc.x < -1.0 || shadow_ndc.x > 1.0 ||
        shadow_ndc.y < -1.0 || shadow_ndc.y > 1.0 ||
        shadow_ndc.z < 0.0 || shadow_ndc.z > 1.0) {
        return 1.0;
    }

    let shadow_uv = vec2<f32>(
        shadow_ndc.x * 0.5 + 0.5,
        -shadow_ndc.y * 0.5 + 0.5
    );
    let current_depth = shadow_ndc.z;

    let texel_size = 1.0 / 2048.0;
    var shadow = 0.0;
    let offsets = array<vec2<f32>, 4>(
        vec2<f32>(-0.65, -0.65),
        vec2<f32>(0.65, -0.65),
        vec2<f32>(-0.65, 0.65),
        vec2<f32>(0.65, 0.65)
    );

    for (var i = 0; i < 4; i++) {
        let sample_uv = shadow_uv + offsets[i] * texel_size * 1.5;
        shadow += textureSampleCompare(t_shadow, s_shadow, sample_uv, current_depth);
    }
    return shadow * 0.25;
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) ao: f32,
    @location(4) uv: vec2<f32>,
    @location(5) tex_id: u32,
    @location(6) material_params: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) ao: f32,
    @location(4) uv: vec2<f32>,
    @location(5) @interpolate(flat) tex_id: u32,
    @location(6) material_params: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = scene.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    out.normal = in.normal;
    out.world_pos = in.position;
    out.ao = in.ao;
    out.uv = in.uv;
    out.tex_id = in.tex_id;
    out.material_params = in.material_params;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var albedo = in.color;
    if (in.tex_id > 0u) {
        let tex_col = textureSample(t_atlas, s_atlas, in.uv, in.tex_id - 1u);
        if (tex_col.a < 0.1) {
            discard;
        }
        albedo = albedo * tex_col.rgb;
    }

    let roughness = clamp(in.material_params.x, 0.04, 0.98);
    let metallic = clamp(in.material_params.y, 0.0, 1.0);
    let shading_model = u32(round(in.material_params.z));
    let sss_strength = in.material_params.w;

    let N = normalize(in.normal);
    let L = normalize(scene.sun_dir.xyz);
    let V = normalize(scene.camera_pos.xyz - in.world_pos);
    let H = normalize(V + L);

    let ndotl = max(dot(N, L), 0.0);
    let ndotv = max(dot(N, V), 0.001);
    let ndoth = max(dot(N, H), 0.0);
    let vdoth = max(dot(V, H), 0.0);

    // Volumetric Minecraft/BSL face lighting weighting (for crisp 3D shape definition):
    var face_weight = 1.0;
    if (N.y > 0.5) {
        face_weight = 1.0;
    } else if (N.y < -0.5) {
        face_weight = 0.52;
    } else if (abs(N.x) > 0.5) {
        face_weight = 0.82;
    } else {
        face_weight = 0.68;
    }

    // Dynamic Cascaded Shadow factor (0.0 in shadow, 1.0 in sun)
    let shadow_factor = calculate_shadow(in.world_pos, N);
    let sun_light = scene.sun_color.rgb * scene.sun_dir.w * 0.80;

    // BSL / Complementary style hemisphere ambient:
    let sky_up = max(N.y, 0.0);
    let ground_down = max(-N.y, 0.0);
    let side_term = max(1.0 - abs(N.y), 0.0);
    let ambient = scene.ambient_sky.rgb * (sky_up * 0.75 + side_term * 0.50)
                + scene.ambient_ground.rgb * (ground_down * 0.60 + side_term * 0.35);

    let ao_val = pow(in.ao, 1.35);
    var lit_color = vec3<f32>(0.0);

    if (shading_model == 4u) {
        // --- 4. Realistic Shader Water (Waves, Fresnel reflection, caustics, and foam) ---
        let time = scene.sky_zenith.w;
        let wave1 = sin(in.world_pos.x * 0.95 + in.world_pos.z * 0.85 + time * 2.2) * 0.08;
        let wave2 = cos(in.world_pos.x * 1.65 - in.world_pos.z * 1.45 + time * 3.1) * 0.06;
        let wave_norm = normalize(vec3<f32>(wave1 + wave2, 1.0, wave1 - wave2));
        let water_N = normalize(mix(N, wave_norm, 0.40));

        let water_ndotl = max(dot(water_N, L), 0.0);
        let water_ndotv = max(dot(water_N, V), 0.001);
        let water_H = normalize(V + L);
        let water_ndoth = max(dot(water_N, water_H), 0.0);

        let shallow_color = vec3<f32>(0.07, 0.45, 0.65);
        let deep_color = vec3<f32>(0.02, 0.15, 0.35);
        let water_base = mix(shallow_color, deep_color, 0.55);

        let fresnel = pow(1.0 - water_ndotv, 4.0);
        let sky_reflection = mix(scene.ambient_sky.rgb * 1.25, scene.sun_color.rgb, 0.30);

        let spec_power = pow(water_ndoth, 96.0) * 1.65 * shadow_factor;
        let water_sun = sun_light * water_ndotl * shadow_factor;

        let foam = pow(clamp(wave1 + wave2 + 0.05, 0.0, 1.0), 3.0) * 0.30;

        let final_water = mix(water_base * (ambient + water_sun), sky_reflection, fresnel * 0.82)
                        + vec3<f32>(foam) + scene.sun_color.rgb * spec_power;

        lit_color = final_water;
    } else if (shading_model == 3u) {
        // --- 1. Emissive (Lamps, glowing ores, torches, lava) ---
        lit_color = albedo * 1.35 + scene.sun_color.rgb * 0.15;
    } else if (shading_model == 1u) {
        // --- 2. Stylized / Cel (Unturned style) ---
        var band = 0.30;
        let lit_val = ndotl * shadow_factor;
        if (lit_val > 0.55) {
            band = 1.0;
        } else if (lit_val > 0.15) {
            band = 0.65;
        }
        let stylized_direct = albedo * sun_light * band;
        let stylized_ambient = albedo * (ambient * 0.85 + vec3<f32>(0.10));
        let rim = pow(1.0 - ndotv, 3.0) * 0.20;
        lit_color = (stylized_ambient + stylized_direct + scene.ambient_sky.rgb * rim) * face_weight * ao_val;
    } else {
        // --- 3. Standard PBR & Foliage ---
        let F0 = mix(vec3<f32>(0.04), albedo, metallic);
        let F = F0 + (vec3<f32>(1.0) - F0) * pow(clamp(1.0 - vdoth, 0.0, 1.0), 5.0);

        let alpha = roughness * roughness;
        let alpha2 = alpha * alpha;
        let denom = ndoth * ndoth * (alpha2 - 1.0) + 1.0;
        let D = alpha2 / (3.14159265 * denom * denom);

        let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
        let G_v = ndotv / (ndotv * (1.0 - k) + k);
        let G_l = ndotl / (ndotl * (1.0 - k) + k);
        let G = G_v * G_l;

        let specular = (D * F * G) / max(4.0 * ndotv * ndotl, 0.001);
        let kD = (vec3<f32>(1.0) - F) * (1.0 - metallic);
        let diffuse = kD * albedo;

        var foliage_trans = vec3<f32>(0.0);
        if (shading_model == 2u) {
            let eye_to_sun = max(dot(V, -L), 0.0);
            let sss = pow(eye_to_sun, 3.0) * max(sss_strength, 0.45);
            foliage_trans = albedo * scene.sun_color.rgb * sss * shadow_factor;
        }

        let direct = (diffuse + specular) * sun_light * ndotl * shadow_factor + foliage_trans;
        let rim = pow(1.0 - ndotv, 4.0) * (1.0 - roughness) * 0.20;
        let ambient_term = albedo * (ambient + scene.ambient_sky.rgb * rim);

        lit_color = (ambient_term + direct) * face_weight * ao_val;
    }

    // Atmospheric distance fog (clean near-field, quadratic falloff to horizon)
    let dist = distance(in.world_pos, scene.camera_pos.xyz);
    let fog_start = scene.camera_pos.w * 0.55;
    let fog_end = scene.camera_pos.w;
    let fog_linear = clamp((dist - fog_start) / max(fog_end - fog_start, 1.0), 0.0, 1.0);
    let fog_factor = fog_linear * fog_linear;

    let brightness = scene.fog_color.w;
    let lit_exposed = lit_color * brightness;
    let fogged_color = mix(lit_exposed, scene.fog_color.rgb * brightness, fog_factor);

    var out_alpha = 1.0;
    if (shading_model == 4u) {
        out_alpha = 0.85;
    }

    return vec4<f32>(tonemap_filmic(fogged_color), out_alpha);
}

// Shadow Map Pass
struct ShadowVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) tex_id: u32,
};

@vertex
fn vs_shadow(in: VertexInput) -> ShadowVertexOutput {
    var out: ShadowVertexOutput;
    out.clip_position = scene.light_view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.tex_id = in.tex_id;
    return out;
}

@fragment
fn fs_shadow(in: ShadowVertexOutput) {
    if (in.tex_id > 0u) {
        let tex_col = textureSample(t_atlas, s_atlas, in.uv, in.tex_id - 1u);
        if (tex_col.a < 0.25) {
            discard;
        }
    }
}

// Procedural Sky and Sun Disc
struct SkyVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) view_ray: vec3<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vertex_index: u32) -> SkyVertexOutput {
    var out: SkyVertexOutput;
    var pos = vec2<f32>(-1.0, -1.0);
    if (vertex_index == 1u) {
        pos = vec2<f32>(3.0, -1.0);
    } else if (vertex_index == 2u) {
        pos = vec2<f32>(-1.0, 3.0);
    }
    out.clip_position = vec4<f32>(pos, 1.0, 1.0);

    let unproj = scene.inv_view_proj * vec4<f32>(pos, 1.0, 1.0);
    out.view_ray = unproj.xyz / unproj.w - scene.camera_pos.xyz;
    return out;
}

@fragment
fn fs_sky(in: SkyVertexOutput) -> @location(0) vec4<f32> {
    let V = normalize(in.view_ray);
    let S = normalize(scene.sun_dir.xyz);
    let sun_elevation = S.y;

    // Atmospheric Rayleigh sky dome gradient
    let height = clamp(V.y, -0.05, 1.0);
    let horizon_blend = pow(clamp(1.0 - max(height, 0.0), 0.0, 1.0), 2.2);
    let sky_base = mix(scene.sky_zenith.rgb, scene.fog_color.rgb, horizon_blend);

    // Ground haze below horizon
    let ground_color = scene.ambient_ground.rgb * 0.8;
    let sky_dome = select(sky_base, mix(scene.fog_color.rgb, ground_color, clamp(-V.y * 3.0, 0.0, 1.0)), V.y < 0.0);

    // Sun alignment cos(theta)
    let cos_sun = dot(V, S);

    // 1. Sun disc (sharp, dazzling core)
    let sun_disc_cos = 0.9992;
    let sun_edge = smoothstep(sun_disc_cos - 0.0004, sun_disc_cos + 0.0001, cos_sun);
    let sun_core = scene.sun_color.rgb * (sun_edge * 14.0 * scene.sun_dir.w);

    // 2. Solar corona / Mie scattering halo around the sun
    let corona_wide = pow(max(cos_sun, 0.0), 10.0) * (scene.sun_color.rgb * 0.55 * scene.sun_dir.w);
    let corona_tight = pow(max(cos_sun, 0.0), 80.0) * (scene.sun_color.rgb * 1.6 * scene.sun_dir.w);
    let sun_glow = corona_wide + corona_tight;

    // 3. Moon disc and halo at night (in opposite direction -S)
    let M = -S;
    let cos_moon = dot(V, M);
    let moon_disc_cos = 0.9994;
    let moon_edge = smoothstep(moon_disc_cos - 0.0002, moon_disc_cos + 0.0001, cos_moon);
    let moon_color = vec3<f32>(0.70, 0.82, 0.98);
    let night_fade = clamp(-sun_elevation * 3.5, 0.0, 1.0);
    let moon_core = moon_color * (moon_edge * 5.0 * night_fade);
    let moon_glow = pow(max(cos_moon, 0.0), 28.0) * (moon_color * 0.40 * night_fade);

    // 4. Subtle procedural night stars
    var stars = vec3<f32>(0.0);
    if (sun_elevation < 0.02 && V.y > 0.04) {
        let star_seed = floor(V * 320.0);
        let star_hash = fract(sin(dot(star_seed, vec3<f32>(12.9898, 78.233, 45.164))) * 43758.5453);
        if (star_hash > 0.994) {
            let star_bright = (star_hash - 0.994) / 0.006;
            stars = vec3<f32>(star_bright * 1.6 * clamp(-sun_elevation * 4.0, 0.0, 1.0));
        }
    }

    let raw_sky = sky_dome + sun_glow + sun_core + moon_glow + moon_core + stars;

    // 5. Procedural Dynamic Clouds (soft drifting cumulus layer at altitude Y = 160)
    let time = scene.sky_zenith.w;
    var cloud_color = vec3<f32>(0.0);
    var cloud_alpha = 0.0;

    if (V.y > 0.03) {
        let cloud_alt = 160.0;
        let t_plane = (cloud_alt - max(scene.camera_pos.y, 0.0)) / V.y;
        if (t_plane > 0.0) {
            let world_cloud = scene.camera_pos.xz + V.xz * t_plane;
            let wind = vec2<f32>(time * 1.8, time * 0.9);
            let cloud_uv = (world_cloud + wind) * 0.0016;

            let c_raw = fbm_clouds(cloud_uv);
            let coverage = 0.48;
            let density = smoothstep(coverage, coverage + 0.32, c_raw);

            let day_factor = clamp(sun_elevation * 3.5, 0.0, 1.0);
            let sunset_factor = clamp(1.0 - abs(sun_elevation) * 4.0, 0.0, 1.0);

            // Shading: upper part catches sun, lower part gets soft blue sky ambient
            let sun_shading = clamp(dot(S, vec3<f32>(0.0, 1.0, 0.0)) * 0.5 + 0.5, 0.35, 1.0);
            let day_top = vec3<f32>(1.0, 0.99, 0.98);
            let day_bottom = vec3<f32>(0.72, 0.77, 0.88);
            let sunset_col = vec3<f32>(1.0, 0.58, 0.36);
            let night_col = vec3<f32>(0.12, 0.16, 0.26);

            var base_cloud = mix(night_col, mix(day_bottom, day_top, sun_shading), day_factor);
            base_cloud = mix(base_cloud, sunset_col, sunset_factor * 0.70);

            let horizon_fade = smoothstep(0.04, 0.22, V.y);
            cloud_alpha = density * horizon_fade * 0.88;
            cloud_color = base_cloud;
        }
    }

    let sky_with_clouds = mix(raw_sky, cloud_color, cloud_alpha);
    let brightness = scene.fog_color.w;

    return vec4<f32>(tonemap_filmic(sky_with_clouds * brightness), 1.0);
}

@vertex
fn vs_outline(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return scene.view_proj * vec4<f32>(position, 1.0);
}

@fragment
fn fs_outline() -> @location(0) vec4<f32> {
    return vec4<f32>(0.10, 0.10, 0.10, 1.0);
}
