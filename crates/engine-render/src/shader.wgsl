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
    let is_day = scene.sun_dir.y >= -0.05;
    let L = select(
        normalize(vec3<f32>(-scene.sun_dir.x, max(-scene.sun_dir.y, 0.12), -scene.sun_dir.z)),
        normalize(vec3<f32>(scene.sun_dir.x, max(scene.sun_dir.y, 0.08), scene.sun_dir.z)),
        is_day
    );
    let V = normalize(scene.camera_pos.xyz - in.world_pos);
    let H = normalize(V + L);

    let ndotl = max(dot(N, L), 0.0);
    let ndotv = max(dot(N, V), 0.001);
    let ndoth = max(dot(N, H), 0.0);
    let vdoth = max(dot(V, H), 0.0);

    // Dynamic Cascaded Shadow factor (0.0 in shadow, 1.0 in sun)
    let shadow_factor = calculate_shadow(in.world_pos, N);
    let direct_light = scene.sun_color.rgb * scene.sun_dir.w * 0.85;

    // FastPBR / SEUS style natural hemisphere ambient:
    let sky_up = max(N.y, 0.0);
    let ground_down = max(-N.y, 0.0);
    let side_term = max(1.0 - abs(N.y), 0.0);
    let ambient = scene.ambient_sky.rgb * (sky_up * 0.80 + side_term * 0.55)
                + scene.ambient_ground.rgb * (ground_down * 0.65 + side_term * 0.40);

    let ao_val = pow(in.ao, 1.25);
    var lit_color = vec3<f32>(0.0);

    if (shading_model == 4u) {
        // --- SEUS Renewed Water Model ---
        let time = scene.sky_zenith.w;
        let p = in.world_pos.xz;

        // Multi-octave wave synthesis with dual counter-propagating ripple frequencies
        let w1 = sin(p.x * 0.95 + p.y * 0.65 + time * 1.4) * 0.035;
        let w2 = cos(p.x * 1.85 - p.y * 1.35 + time * 2.1) * 0.022;
        let w3 = sin(-p.x * 3.4 + p.y * 2.7 + time * 3.2) * 0.014;
        let w4 = cos(p.x * 5.2 + p.y * 4.1 - time * 4.0) * 0.008;

        // Wave normal derivatives (SEUS Gerstner surface gradient)
        let dw_dx = (0.95 * cos(p.x * 0.95 + p.y * 0.65 + time * 1.4) * 0.035)
                  - (1.85 * sin(p.x * 1.85 - p.y * 1.35 + time * 2.1) * 0.022)
                  - (3.4 * cos(-p.x * 3.4 + p.y * 2.7 + time * 3.2) * 0.014)
                  - (5.2 * sin(p.x * 5.2 + p.y * 4.1 - time * 4.0) * 0.008);
        let dw_dz = (0.65 * cos(p.x * 0.95 + p.y * 0.65 + time * 1.4) * 0.035)
                  + (1.35 * sin(p.x * 1.85 - p.y * 1.35 + time * 2.1) * 0.022)
                  + (2.7 * cos(-p.x * 3.4 + p.y * 2.7 + time * 3.2) * 0.014)
                  - (4.1 * sin(p.x * 5.2 + p.y * 4.1 - time * 4.0) * 0.008);

        let wave_n = normalize(vec3<f32>(-dw_dx * 2.2, 1.0, -dw_dz * 2.2));
        let water_N = normalize(mix(N, wave_n, 0.65));

        let water_ndotl = max(dot(water_N, L), 0.0);
        let water_ndotv = max(dot(water_N, V), 0.001);
        let water_H = normalize(V + L);
        let water_ndoth = max(dot(water_N, water_H), 0.0);

        // SEUS physical absorption and scattering
        let shallow_water = vec3<f32>(0.12, 0.68, 0.78);
        let deep_water = vec3<f32>(0.02, 0.14, 0.36);
        let water_base = mix(shallow_water, deep_water, 0.45);

        // Physical Schlick Fresnel reflection
        let fresnel = pow(clamp(1.0 - water_ndotv, 0.0, 1.0), 3.8);
        let sky_refl = mix(scene.ambient_sky.rgb * 1.6, scene.sun_color.rgb, 0.32);

        // SEUS microfacet specular glint
        let spec_power = pow(water_ndoth, 196.0) * 3.6 * shadow_factor;
        let water_direct = direct_light * water_ndotl * shadow_factor;

        // Dynamic underwater caustics
        let caustic = pow(sin(p.x * 3.2 + time * 1.6) * sin(p.y * 3.2 - time * 1.6) * 0.5 + 0.5, 3.5) * 0.09;

        let final_water = mix(water_base * (ambient * 0.95 + water_direct * 0.85 + caustic), sky_refl, fresnel * 0.82)
                        + scene.sun_color.rgb * spec_power;

        lit_color = final_water;
    } else if (shading_model == 3u) {
        // --- 1. Emissive (Lamps, glowing ores, torches, lava) ---
        lit_color = albedo * 1.45 + scene.sun_color.rgb * 0.15;
    } else if (shading_model == 1u) {
        // --- 2. Stylized / Cel (Unturned style) ---
        var band = 0.30;
        let lit_val = ndotl * shadow_factor;
        if (lit_val > 0.55) {
            band = 1.0;
        } else if (lit_val > 0.15) {
            band = 0.65;
        }
        let stylized_direct = albedo * direct_light * band;
        let stylized_ambient = albedo * (ambient * 0.85 + vec3<f32>(0.10));
        let rim = pow(1.0 - ndotv, 3.0) * 0.20;
        lit_color = (stylized_ambient + stylized_direct + scene.ambient_sky.rgb * rim) * ao_val;
    } else {
        // --- 3. Standard PBR & Foliage (FastPBR & Vintage Story physical lighting) ---
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

        // Physical directional light with soft wrap for organic voxel edges
        let ndotl_wrap = max((dot(N, L) + 0.12) / 1.12, 0.0);
        let direct = (diffuse * ndotl_wrap + specular * ndotl) * direct_light * shadow_factor + foliage_trans;
        let rim = pow(1.0 - ndotv, 4.0) * (1.0 - roughness) * 0.18;
        let ambient_term = albedo * (ambient + scene.ambient_sky.rgb * rim);

        lit_color = (ambient_term + direct) * ao_val;
    }

    // Insanity Shader Cinematic Exponential Fog with atmospheric forward-scattering
    let dist = distance(in.world_pos, scene.camera_pos.xyz);
    let fog_range = max(scene.camera_pos.w, 16.0);
    let fog_dist_norm = dist / fog_range;
    let fog_factor = clamp(1.0 - exp(-2.2 * pow(fog_dist_norm, 1.25)), 0.0, 1.0);

    let V_frag = normalize(scene.camera_pos.xyz - in.world_pos);
    let vol = max(dot(-V_frag, L), 0.0);
    let forward_glow = pow(vol, 6.0) * 0.28 * scene.sun_dir.w;
    let fog_color_scattered = scene.fog_color.rgb + scene.sun_color.rgb * forward_glow;

    let brightness = scene.fog_color.w;
    let lit_exposed = lit_color * brightness;
    let fogged_color = mix(lit_exposed, fog_color_scattered * brightness, fog_factor);

    var out_alpha = 1.0;
    if (shading_model == 4u) {
        let fresnel_a = pow(clamp(1.0 - max(dot(N, V), 0.001), 0.0, 1.0), 3.0);
        out_alpha = mix(0.62, 0.94, fresnel_a);
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

    // 1. Sun disc (sharp, dazzling core, visible during the day)
    let day_fade = clamp(sun_elevation * 4.0, 0.0, 1.0);
    let sun_disc_cos = 0.9992;
    let sun_edge = smoothstep(sun_disc_cos - 0.0004, sun_disc_cos + 0.0001, cos_sun);
    let sun_core = scene.sun_color.rgb * (sun_edge * 14.0 * scene.sun_dir.w * day_fade);

    // 2. Solar corona / Mie scattering halo around the sun
    let corona_wide = pow(max(cos_sun, 0.0), 10.0) * (scene.sun_color.rgb * 0.55 * scene.sun_dir.w * day_fade);
    let corona_tight = pow(max(cos_sun, 0.0), 80.0) * (scene.sun_color.rgb * 1.6 * scene.sun_dir.w * day_fade);
    let sun_glow = corona_wide + corona_tight;

    // 3. Moon disc and halo at night (in opposite direction -S)
    let M = -S;
    let cos_moon = dot(V, M);
    let moon_disc_cos = 0.9994;
    let moon_edge = smoothstep(moon_disc_cos - 0.0002, moon_disc_cos + 0.0001, cos_moon);
    let moon_color = vec3<f32>(0.72, 0.84, 0.98);
    let night_fade = clamp(-sun_elevation * 3.5, 0.0, 1.0);
    let moon_core = moon_color * (moon_edge * 5.0 * night_fade);
    let moon_glow = pow(max(cos_moon, 0.0), 28.0) * (moon_color * 0.40 * night_fade);

    // 4. FastPBR celestial starfield & Milky Way
    var stars = vec3<f32>(0.0);
    if (night_fade > 0.01 && V.y > 0.01) {
        let time = scene.sky_zenith.w;
        let plane = V.xz / (V.y + length(V.xz) * 0.66);
        let rot = time * 0.010;
        let s_rot = sin(rot);
        let c_rot = cos(rot);
        let rot_plane = vec2<f32>(plane.x * c_rot - plane.y * s_rot, plane.x * s_rot + plane.y * c_rot);

        let uv1 = floor(rot_plane * 520.0) / 520.0;
        let uv2 = floor(rot_plane * 240.0) / 240.0;

        let h1 = fract(sin(dot(uv1, vec2<f32>(12.9898, 78.233))) * 43758.5453);
        let h2 = fract(sin(dot(uv1 + vec2<f32>(0.13, 0.37), vec2<f32>(93.185, 49.321))) * 23421.631);
        let h3 = fract(sin(dot(uv2, vec2<f32>(41.139, 17.583))) * 57234.129);

        let blue_star = vec3<f32>(0.65, 0.82, 1.0);
        let warm_star = vec3<f32>(1.0, 0.78, 0.58);
        let star_col = mix(blue_star, warm_star, h2);

        var star_val = 0.0;
        if (h1 > 0.982) {
            star_val = (h1 - 0.982) / 0.018 * 1.8;
        }
        if (h3 > 0.992) {
            star_val = star_val + (h3 - 0.992) / 0.008 * 3.2;
        }

        let twinkle = sin(time * 3.5 + h1 * 40.0) * 0.18 + 0.82;

        // Soft Milky Way band
        let milky_coord = rot_plane.x * 0.6 + rot_plane.y * 0.8;
        let milky_way = exp(-milky_coord * milky_coord * 0.8) * 0.12 * clamp(V.y * 2.0, 0.0, 1.0);
        let milky_col = vec3<f32>(0.55, 0.65, 0.85) * milky_way;

        stars = (star_col * star_val * twinkle + milky_col) * night_fade * smoothstep(0.02, 0.18, V.y);
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
