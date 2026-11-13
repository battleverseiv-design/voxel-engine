use engine_core::AvatarModel;
use engine_world::VoxelVertex;
use glam::{Mat3, Vec3};

#[inline]
fn shade(c: [f32; 3], f: f32) -> [f32; 3] {
    [
        (c[0] * f).clamp(0.0, 1.0),
        (c[1] * f).clamp(0.0, 1.0),
        (c[2] * f).clamp(0.0, 1.0),
    ]
}

#[inline]
fn mono_color(col: [f32; 3]) -> [[f32; 3]; 6] {
    [
        shade(col, 1.10), // Top (+Y)
        shade(col, 0.68), // Bottom (-Y)
        col,              // North (-Z)
        shade(col, 0.88), // South (+Z)
        shade(col, 0.84), // East (+X)
        shade(col, 0.84), // West (-X)
    ]
}

fn push_box(
    out: &mut Vec<VoxelVertex>,
    center: Vec3,
    half_size: Vec3,
    rot: Mat3,
    colors: [[f32; 3]; 6], // Top (+Y), Bottom (-Y), North (-Z), South (+Z), East (+X), West (-X)
) {
    let dx = half_size.x;
    let dy = half_size.y;
    let dz = half_size.z;

    let faces: [(Vec3, [Vec3; 4]); 6] = [
        // Top (+Y)
        (
            Vec3::Y,
            [
                Vec3::new(-dx, dy, -dz),
                Vec3::new(-dx, dy, dz),
                Vec3::new(dx, dy, dz),
                Vec3::new(dx, dy, -dz),
            ],
        ),
        // Bottom (-Y)
        (
            Vec3::NEG_Y,
            [
                Vec3::new(-dx, -dy, dz),
                Vec3::new(-dx, -dy, -dz),
                Vec3::new(dx, -dy, -dz),
                Vec3::new(dx, -dy, dz),
            ],
        ),
        // North (-Z)
        (
            Vec3::NEG_Z,
            [
                Vec3::new(dx, dy, -dz),
                Vec3::new(dx, -dy, -dz),
                Vec3::new(-dx, -dy, -dz),
                Vec3::new(-dx, dy, -dz),
            ],
        ),
        // South (+Z)
        (
            Vec3::Z,
            [
                Vec3::new(-dx, dy, dz),
                Vec3::new(-dx, -dy, dz),
                Vec3::new(dx, -dy, dz),
                Vec3::new(dx, dy, dz),
            ],
        ),
        // East (+X)
        (
            Vec3::X,
            [
                Vec3::new(dx, dy, dz),
                Vec3::new(dx, -dy, dz),
                Vec3::new(dx, -dy, -dz),
                Vec3::new(dx, dy, -dz),
            ],
        ),
        // West (-X)
        (
            Vec3::NEG_X,
            [
                Vec3::new(-dx, dy, -dz),
                Vec3::new(-dx, -dy, -dz),
                Vec3::new(-dx, -dy, dz),
                Vec3::new(-dx, dy, dz),
            ],
        ),
    ];

    for (face_idx, (local_norm, corners)) in faces.iter().enumerate() {
        let world_norm = (rot * (*local_norm)).normalize();
        let c = [
            center + rot * corners[0],
            center + rot * corners[1],
            center + rot * corners[2],
            center + rot * corners[3],
        ];
        let color = colors[face_idx];

        let tri = [0, 1, 2, 0, 2, 3];
        for &idx in &tri {
            out.push(VoxelVertex {
                position: c[idx].to_array(),
                normal: world_norm.to_array(),
                color,
                ao: 1.0,
                uv: [0.0, 0.0],
                tex_id: 0,
                material_params: [0.85, 0.0, 1.0, 0.0],
            });
        }
    }
}

pub fn push_textured_box(
    out: &mut Vec<VoxelVertex>,
    center: Vec3,
    half_size: Vec3,
    rot: Mat3,
    colors: [[f32; 3]; 6],
    textures: [u16; 6],
) {
    let dx = half_size.x;
    let dy = half_size.y;
    let dz = half_size.z;

    let faces: [(Vec3, [Vec3; 4]); 6] = [
        (
            Vec3::Y,
            [
                Vec3::new(-dx, dy, -dz),
                Vec3::new(-dx, dy, dz),
                Vec3::new(dx, dy, dz),
                Vec3::new(dx, dy, -dz),
            ],
        ),
        (
            Vec3::NEG_Y,
            [
                Vec3::new(-dx, -dy, dz),
                Vec3::new(-dx, -dy, -dz),
                Vec3::new(dx, -dy, -dz),
                Vec3::new(dx, -dy, dz),
            ],
        ),
        (
            Vec3::NEG_Z,
            [
                Vec3::new(dx, dy, -dz),
                Vec3::new(dx, -dy, -dz),
                Vec3::new(-dx, -dy, -dz),
                Vec3::new(-dx, dy, -dz),
            ],
        ),
        (
            Vec3::Z,
            [
                Vec3::new(-dx, dy, dz),
                Vec3::new(-dx, -dy, dz),
                Vec3::new(dx, -dy, dz),
                Vec3::new(dx, dy, dz),
            ],
        ),
        (
            Vec3::X,
            [
                Vec3::new(dx, dy, dz),
                Vec3::new(dx, -dy, dz),
                Vec3::new(dx, -dy, -dz),
                Vec3::new(dx, dy, -dz),
            ],
        ),
        (
            Vec3::NEG_X,
            [
                Vec3::new(-dx, dy, -dz),
                Vec3::new(-dx, -dy, -dz),
                Vec3::new(-dx, -dy, dz),
                Vec3::new(-dx, dy, dz),
            ],
        ),
    ];

    let quad_uvs = [
        [0.0, 0.0],
        [0.0, 1.0],
        [1.0, 1.0],
        [1.0, 0.0],
    ];

    for (face_idx, (local_norm, corners)) in faces.iter().enumerate() {
        let world_norm = (rot * (*local_norm)).normalize();
        let c = [
            center + rot * corners[0],
            center + rot * corners[1],
            center + rot * corners[2],
            center + rot * corners[3],
        ];
        let color = colors[face_idx];
        let tex_id = textures[face_idx] as u32;

        let tri = [0, 1, 2, 0, 2, 3];
        for &idx in &tri {
            out.push(VoxelVertex {
                position: c[idx].to_array(),
                normal: world_norm.to_array(),
                color,
                ao: 1.0,
                uv: quad_uvs[idx],
                tex_id,
                material_params: [0.85, 0.0, 1.0, 0.0],
            });
        }
    }
}

/// Builds dynamic stylized survivor avatar mesh (Variant 2: Unturned II / Hytale / Project Zomboid).
/// Features articulated limbs with elbows and knees, boots with soles/toes, V-shaped torso, neck, and volumetric 3D hair strands.
pub fn build_avatar_mesh(
    avatar: &AvatarModel,
    feet_pos: Vec3,
    yaw: f32,
    pitch: f32,
    walk_phase: f32,
    is_moving: bool,
    horizontal_speed: f32,
    is_crouching: bool,
    is_in_air: bool,
    is_swimming: bool,
    arm_swing: f32,
    held_block_colors: Option<[[f32; 3]; 6]>,
    held_block_textures: Option<[u16; 6]>,
) -> Vec<VoxelVertex> {
    let mut vertices = Vec::with_capacity(650);

    let (skin, shirt, pants, hair) = match avatar {
        AvatarModel::None => return vertices,
        AvatarModel::SimpleHumanoid {
            skin_color,
            shirt_color,
            pants_color,
            hair_color,
        } => (*skin_color, *shirt_color, *pants_color, *hair_color),
    };

    // Derived theme colors for tactical rugged survivor
    let belt_color = [0.18, 0.13, 0.09];
    let buckle_color = [0.82, 0.80, 0.76];
    let collar_color = shade(shirt, 1.15);
    let cuff_color = shade(shirt, 0.82);
    let boot_color = [0.19, 0.15, 0.11];
    let sole_color = [0.09, 0.08, 0.07];

    // Engine orientation: forward = (sin_yaw, 0, -cos_yaw)
    let (sin_y, cos_y) = yaw.sin_cos();
    let body_forward = Vec3::new(sin_y, 0.0, -cos_y).normalize();
    let body_right = Vec3::new(cos_y, 0.0, sin_y).normalize();
    let body_up = Vec3::Y;

    // Movement parameters
    let speed_ratio = (horizontal_speed / 4.8).clamp(0.0, 2.0);
    let is_sprinting = horizontal_speed > 5.2 && is_moving;

    // Dynamic crouching height drop & posture
    let height_drop = if is_crouching { 0.44 } else { 0.0 };
    let base_feet = feet_pos - body_up * height_drop;

    // Root rotation matrix
    let mut root_rot = Mat3::from_cols(body_right, body_up, -body_forward);

    if is_swimming {
        // Horizontal swimming posture
        root_rot = root_rot * Mat3::from_rotation_x(-1.25);
    }

    // Walking vertical bobbing & hip sway
    let bob_amp = if is_moving { speed_ratio.min(1.2) } else { 0.15 };
    let hip_bounce = if is_moving && !is_in_air && !is_swimming {
        (walk_phase * 2.0).sin().abs() * 0.024 * bob_amp
    } else {
        0.0
    };
    let hip_sway = if is_moving && !is_in_air && !is_swimming {
        (walk_phase).sin() * 0.05 * speed_ratio
    } else {
        0.0
    };

    let pelvis_rot = root_rot * Mat3::from_rotation_y(hip_sway);
    let pelvis_center = base_feet + body_up * (0.84 - hip_bounce);

    // --- 1. PELVIS & BELT ---
    push_box(
        &mut vertices,
        pelvis_center,
        Vec3::new(0.17, 0.085, 0.11),
        pelvis_rot,
        mono_color(pants),
    );
    // Belt strip around waist
    let belt_center = pelvis_center + pelvis_rot * Vec3::new(0.0, 0.055, 0.0);
    push_box(
        &mut vertices,
        belt_center,
        Vec3::new(0.174, 0.032, 0.114),
        pelvis_rot,
        mono_color(belt_color),
    );
    // 3D Belt Buckle
    let buckle_center = pelvis_center + pelvis_rot * Vec3::new(0.0, 0.055, -0.118);
    push_box(
        &mut vertices,
        buckle_center,
        Vec3::new(0.04, 0.026, 0.008),
        pelvis_rot,
        mono_color(buckle_color),
    );

    // --- 2. LEGS & ARTICULATED KNEES & BOOTS ---
    let (leg_swing_r, leg_swing_l, knee_bend_r, knee_bend_l) = if is_swimming {
        let kick = (walk_phase * 1.5).sin() * 0.45;
        (kick, -kick, (-kick * 0.8).max(0.0), (kick * 0.8).max(0.0))
    } else if is_in_air {
        // Athletic tucked jump pose
        (0.42, 0.42, -0.58, -0.58)
    } else if is_crouching {
        // Deep tactical crouch bend
        (0.72, 0.72, -0.92, -0.92)
    } else if is_moving {
        let amp = 0.52 * speed_ratio.clamp(0.35, 1.35);
        let sw = (walk_phase).sin() * amp;
        let kr = if sw < 0.0 { -sw * 1.25 } else { -sw * 0.18 };
        let kl = if -sw < 0.0 { sw * 1.25 } else { sw * 0.18 };
        (sw, -sw, -kr, -kl)
    } else {
        // Natural relaxed stance (slight knee unlock)
        (0.03, 0.03, -0.05, -0.05)
    };

    let hip_y = 0.02;
    let thigh_len = 0.36;
    let shin_len = 0.36;

    // Right Leg
    let r_hip_pos = pelvis_center + pelvis_rot * Vec3::new(0.10, hip_y, 0.0);
    let r_thigh_rot = pelvis_rot * Mat3::from_rotation_x(leg_swing_r);
    let r_thigh_center = r_hip_pos + r_thigh_rot * Vec3::new(0.0, -thigh_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        r_thigh_center,
        Vec3::new(0.062, thigh_len * 0.5, 0.072),
        r_thigh_rot,
        mono_color(pants),
    );

    let r_knee_pos = r_hip_pos + r_thigh_rot * Vec3::new(0.0, -thigh_len, 0.0);
    let r_shin_rot = r_thigh_rot * Mat3::from_rotation_x(knee_bend_r);
    let r_shin_center = r_knee_pos + r_shin_rot * Vec3::new(0.0, -shin_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        r_shin_center,
        Vec3::new(0.055, shin_len * 0.5, 0.065),
        r_shin_rot,
        mono_color(pants),
    );

    // Right Boot with Ankle, Toe Box & Rubber Sole
    let r_ankle_pos = r_knee_pos + r_shin_rot * Vec3::new(0.0, -shin_len, 0.0);
    // Boot Main Body
    push_box(
        &mut vertices,
        r_ankle_pos + r_shin_rot * Vec3::new(0.0, 0.06, 0.0),
        Vec3::new(0.058, 0.065, 0.072),
        r_shin_rot,
        mono_color(boot_color),
    );
    // Protruding Toe Box (-Z forward)
    push_box(
        &mut vertices,
        r_ankle_pos + r_shin_rot * Vec3::new(0.0, 0.045, -0.09),
        Vec3::new(0.055, 0.045, 0.055),
        r_shin_rot,
        mono_color(boot_color),
    );
    // Tread Sole
    push_box(
        &mut vertices,
        r_ankle_pos + r_shin_rot * Vec3::new(0.0, 0.012, -0.045),
        Vec3::new(0.062, 0.014, 0.115),
        r_shin_rot,
        mono_color(sole_color),
    );

    // Left Leg
    let l_hip_pos = pelvis_center + pelvis_rot * Vec3::new(-0.10, hip_y, 0.0);
    let l_thigh_rot = pelvis_rot * Mat3::from_rotation_x(leg_swing_l);
    let l_thigh_center = l_hip_pos + l_thigh_rot * Vec3::new(0.0, -thigh_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        l_thigh_center,
        Vec3::new(0.062, thigh_len * 0.5, 0.072),
        l_thigh_rot,
        mono_color(pants),
    );

    let l_knee_pos = l_hip_pos + l_thigh_rot * Vec3::new(0.0, -thigh_len, 0.0);
    let l_shin_rot = l_thigh_rot * Mat3::from_rotation_x(knee_bend_l);
    let l_shin_center = l_knee_pos + l_shin_rot * Vec3::new(0.0, -shin_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        l_shin_center,
        Vec3::new(0.055, shin_len * 0.5, 0.065),
        l_shin_rot,
        mono_color(pants),
    );

    // Left Boot
    let l_ankle_pos = l_knee_pos + l_shin_rot * Vec3::new(0.0, -shin_len, 0.0);
    push_box(
        &mut vertices,
        l_ankle_pos + l_shin_rot * Vec3::new(0.0, 0.06, 0.0),
        Vec3::new(0.058, 0.065, 0.072),
        l_shin_rot,
        mono_color(boot_color),
    );
    push_box(
        &mut vertices,
        l_ankle_pos + l_shin_rot * Vec3::new(0.0, 0.045, -0.09),
        Vec3::new(0.055, 0.045, 0.055),
        l_shin_rot,
        mono_color(boot_color),
    );
    push_box(
        &mut vertices,
        l_ankle_pos + l_shin_rot * Vec3::new(0.0, 0.012, -0.045),
        Vec3::new(0.062, 0.014, 0.115),
        l_shin_rot,
        mono_color(sole_color),
    );

    // --- 3. V-SHAPED CHEST & TORSO ---
    let torso_tilt = if is_crouching {
        0.42
    } else if is_sprinting {
        0.24
    } else {
        (walk_phase * 0.8).sin() * 0.018 // Breathing
    };
    let chest_twist = if is_moving && !is_in_air {
        -hip_sway * 0.65
    } else {
        0.0
    };

    // Mining / Attacking / Placing action swing dynamics
    let p = arm_swing.clamp(0.0, 1.0);
    let s_fast = (p.sqrt() * std::f32::consts::PI).sin();
    let s_full = (p * std::f32::consts::PI).sin();

    let swing_chest_twist = if p > 0.001 { -s_full * 0.22 } else { 0.0 };
    let swing_chest_pitch = if p > 0.001 { s_fast * 0.10 } else { 0.0 };

    let chest_rot = pelvis_rot
        * Mat3::from_rotation_y(chest_twist + swing_chest_twist)
        * Mat3::from_rotation_x(torso_tilt + swing_chest_pitch);

    let chest_center = pelvis_center + chest_rot * Vec3::new(0.0, 0.26, 0.0);
    push_box(
        &mut vertices,
        chest_center,
        Vec3::new(0.19, 0.18, 0.12),
        chest_rot,
        mono_color(shirt),
    );

    // 3D Shirt Collars (V-Neck styling)
    let collar_l = chest_center + chest_rot * Vec3::new(-0.08, 0.165, -0.124);
    let collar_r = chest_center + chest_rot * Vec3::new(0.08, 0.165, -0.124);
    push_box(
        &mut vertices,
        collar_l,
        Vec3::new(0.045, 0.038, 0.012),
        chest_rot,
        mono_color(collar_color),
    );
    push_box(
        &mut vertices,
        collar_r,
        Vec3::new(0.045, 0.038, 0.012),
        chest_rot,
        mono_color(collar_color),
    );

    // Neck
    let neck_center = chest_center + chest_rot * Vec3::new(0.0, 0.205, 0.0);
    push_box(
        &mut vertices,
        neck_center,
        Vec3::new(0.058, 0.032, 0.058),
        chest_rot,
        mono_color(skin),
    );

    // --- 4. HEAD, EYES & 3D HAIR (Hytale / Unturned II Style) ---
    let head_rot = chest_rot * Mat3::from_rotation_x(pitch);
    let head_center = neck_center + head_rot * Vec3::new(0.0, 0.16, 0.0);
    push_box(
        &mut vertices,
        head_center,
        Vec3::new(0.13, 0.13, 0.13),
        head_rot,
        mono_color(skin),
    );

    // Expressive Unturned II / Hytale Eyes (-Z front)
    let eye_white = [0.96, 0.97, 0.99];
    let eye_iris = [0.18, 0.42, 0.60];
    let eye_pupil = [0.08, 0.10, 0.15];
    let eye_shine = [1.0, 1.0, 1.0];

    for &side_mult in &[-1.0f32, 1.0f32] {
        let eye_x = side_mult * 0.058;
        let eye_pos = head_center + head_rot * Vec3::new(eye_x, 0.012, -0.132);
        // Sclera
        push_box(
            &mut vertices,
            eye_pos,
            Vec3::new(0.034, 0.030, 0.004),
            head_rot,
            mono_color(eye_white),
        );
        // Iris & Pupil
        push_box(
            &mut vertices,
            eye_pos + head_rot * Vec3::new(side_mult * 0.006, -0.002, -0.002),
            Vec3::new(0.020, 0.024, 0.004),
            head_rot,
            mono_color(eye_iris),
        );
        push_box(
            &mut vertices,
            eye_pos + head_rot * Vec3::new(side_mult * 0.006, 0.0, -0.003),
            Vec3::new(0.012, 0.014, 0.004),
            head_rot,
            mono_color(eye_pupil),
        );
        // Catchlight gloss
        push_box(
            &mut vertices,
            eye_pos + head_rot * Vec3::new(side_mult * 0.002, 0.010, -0.004),
            Vec3::new(0.007, 0.007, 0.003),
            head_rot,
            mono_color(eye_shine),
        );
    }

    // Volumetric 3D Hair Strands
    // Top Crown
    let hair_crown = head_center + head_rot * Vec3::new(0.0, 0.115, 0.01);
    push_box(
        &mut vertices,
        hair_crown,
        Vec3::new(0.138, 0.032, 0.142),
        head_rot,
        mono_color(hair),
    );
    // Back Hair
    let hair_back = head_center + head_rot * Vec3::new(0.0, -0.01, 0.125);
    push_box(
        &mut vertices,
        hair_back,
        Vec3::new(0.136, 0.115, 0.024),
        head_rot,
        mono_color(hair),
    );
    // Left & Right Bangs (asymmetric front tufts)
    let bang_l = head_center + head_rot * Vec3::new(-0.065, 0.095, -0.136);
    let bang_r = head_center + head_rot * Vec3::new(0.062, 0.088, -0.136);
    push_box(
        &mut vertices,
        bang_l,
        Vec3::new(0.058, 0.032, 0.015),
        head_rot,
        mono_color(hair),
    );
    push_box(
        &mut vertices,
        bang_r,
        Vec3::new(0.048, 0.026, 0.015),
        head_rot,
        mono_color(hair),
    );
    // Sideburns
    let side_l = head_center + head_rot * Vec3::new(-0.135, 0.02, 0.0);
    let side_r = head_center + head_rot * Vec3::new(0.135, 0.02, 0.0);
    push_box(
        &mut vertices,
        side_l,
        Vec3::new(0.014, 0.075, 0.075),
        head_rot,
        mono_color(hair),
    );
    push_box(
        &mut vertices,
        side_r,
        Vec3::new(0.014, 0.075, 0.075),
        head_rot,
        mono_color(hair),
    );

    // --- 5. ARTICULATED ARMS (Shoulder, Upper Arm, Rolled Cuff, Forearm, Hand) ---
    let (arm_swing_r, arm_swing_l, elbow_bend_r, elbow_bend_l) = if is_swimming {
        let arm_sw = (walk_phase * 1.5).cos() * 0.45;
        (arm_sw, -arm_sw, 0.45, 0.45)
    } else if is_in_air {
        (-0.35, -0.35, 0.55, 0.55)
    } else if is_crouching {
        (0.20, 0.20, 0.65, 0.65)
    } else if is_moving {
        let sw_r = -leg_swing_r * 0.85;
        let sw_l = -leg_swing_l * 0.85;
        let base_elbow = if is_sprinting { 0.85 } else { 0.22 };
        let elb_r = base_elbow + (sw_r.max(0.0) * 0.95);
        let elb_l = base_elbow + (sw_l.max(0.0) * 0.95);
        (sw_r, sw_l, elb_r, elb_l)
    } else {
        (0.08, 0.08, 0.22, 0.22) // Relaxed human posture
    };

    let shoulder_y = 0.13;
    let upper_len = 0.23;
    let fore_len = 0.21;
    let hand_len = 0.08;

    // Right Arm
    let r_shoulder = chest_center + chest_rot * Vec3::new(0.25, shoulder_y, 0.0);
    let r_air_roll = if is_in_air { -0.28 } else { -0.06 };
    let r_up_angle = if p > 0.001 {
        // Mining / placing chop curve: lifts arm on windup, slams down forward on strike
        let chop = -0.55 * (1.0 - p).powi(2) + 0.95 * s_fast;
        chop - arm_swing_r * 0.15
    } else if held_block_colors.is_some() {
        0.52 - arm_swing_r * 0.25
    } else {
        arm_swing_r
    };
    let r_swing_roll = if p > 0.001 { -s_fast * 0.22 } else { 0.0 };
    let r_up_rot = chest_rot
        * Mat3::from_rotation_z(r_air_roll + r_swing_roll)
        * Mat3::from_rotation_x(r_up_angle);
    let r_up_center = r_shoulder + r_up_rot * Vec3::new(0.0, -upper_len * 0.5, 0.0);
    // Upper Arm Sleeve
    push_box(
        &mut vertices,
        r_up_center,
        Vec3::new(0.052, upper_len * 0.5, 0.058),
        r_up_rot,
        mono_color(shirt),
    );
    // Rolled Cuff
    push_box(
        &mut vertices,
        r_shoulder + r_up_rot * Vec3::new(0.0, -upper_len + 0.02, 0.0),
        Vec3::new(0.056, 0.024, 0.062),
        r_up_rot,
        mono_color(cuff_color),
    );

    // Elbow & Forearm
    let r_elbow_pos = r_shoulder + r_up_rot * Vec3::new(0.0, -upper_len, 0.0);
    let r_elbow_bend = if p > 0.001 {
        0.30 + s_full * 0.90
    } else {
        elbow_bend_r
    };
    let r_fore_rot = r_up_rot * Mat3::from_rotation_x(r_elbow_bend);
    let r_fore_center = r_elbow_pos + r_fore_rot * Vec3::new(0.0, -fore_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        r_fore_center,
        Vec3::new(0.046, fore_len * 0.5, 0.050),
        r_fore_rot,
        mono_color(skin),
    );

    // Hand & Held Block
    let r_wrist_pos = r_elbow_pos + r_fore_rot * Vec3::new(0.0, -fore_len, 0.0);
    let r_hand_center = r_wrist_pos + r_fore_rot * Vec3::new(0.0, -hand_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        r_hand_center,
        Vec3::new(0.044, hand_len * 0.5, 0.046),
        r_fore_rot,
        mono_color(skin),
    );

    if let Some(block_colors) = held_block_colors {
        let block_pos = r_hand_center + r_fore_rot * Vec3::new(0.0, -0.04, -0.08);
        if let Some(textures) = held_block_textures {
            push_textured_box(
                &mut vertices,
                block_pos,
                Vec3::splat(0.085),
                r_fore_rot,
                block_colors,
                textures,
            );
        } else {
            push_box(
                &mut vertices,
                block_pos,
                Vec3::splat(0.085),
                r_fore_rot,
                block_colors,
            );
        }
    }

    // Left Arm
    let l_shoulder = chest_center + chest_rot * Vec3::new(-0.25, shoulder_y, 0.0);
    let l_air_roll = if is_in_air { 0.28 } else { 0.06 };
    let l_up_rot = chest_rot
        * Mat3::from_rotation_z(l_air_roll)
        * Mat3::from_rotation_x(arm_swing_l);
    let l_up_center = l_shoulder + l_up_rot * Vec3::new(0.0, -upper_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        l_up_center,
        Vec3::new(0.052, upper_len * 0.5, 0.058),
        l_up_rot,
        mono_color(shirt),
    );
    push_box(
        &mut vertices,
        l_shoulder + l_up_rot * Vec3::new(0.0, -upper_len + 0.02, 0.0),
        Vec3::new(0.056, 0.024, 0.062),
        l_up_rot,
        mono_color(cuff_color),
    );

    let l_elbow_pos = l_shoulder + l_up_rot * Vec3::new(0.0, -upper_len, 0.0);
    let l_fore_rot = l_up_rot * Mat3::from_rotation_x(elbow_bend_l);
    let l_fore_center = l_elbow_pos + l_fore_rot * Vec3::new(0.0, -fore_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        l_fore_center,
        Vec3::new(0.046, fore_len * 0.5, 0.050),
        l_fore_rot,
        mono_color(skin),
    );

    let l_wrist_pos = l_elbow_pos + l_fore_rot * Vec3::new(0.0, -fore_len, 0.0);
    let l_hand_center = l_wrist_pos + l_fore_rot * Vec3::new(0.0, -hand_len * 0.5, 0.0);
    push_box(
        &mut vertices,
        l_hand_center,
        Vec3::new(0.044, hand_len * 0.5, 0.046),
        l_fore_rot,
        mono_color(skin),
    );

    vertices
}

/// Builds the first-person arm and held item mesh with walking bob, swing, and weapon sway.
pub fn build_first_person_arm_mesh(
    avatar: &AvatarModel,
    camera_pos: Vec3,
    camera_yaw: f32,
    camera_pitch: f32,
    walk_phase: f32,
    is_moving: bool,
    swing_progress: f32,
    sway_offset: Vec3,
    held_block_colors: Option<[[f32; 3]; 6]>,
    held_block_textures: Option<[u16; 6]>,
) -> Vec<VoxelVertex> {
    let mut vertices = Vec::with_capacity(160);

    let (skin, shirt) = match avatar {
        AvatarModel::None => return vertices,
        AvatarModel::SimpleHumanoid {
            skin_color,
            shirt_color,
            ..
        } => (*skin_color, *shirt_color),
    };

    let cuff_color = shade(shirt, 0.82);

    // Camera view basis
    let (sin_y, cos_y) = camera_yaw.sin_cos();
    let (sin_p, cos_p) = camera_pitch.sin_cos();
    let cam_fwd = Vec3::new(cos_p * sin_y, sin_p, -cos_p * cos_y).normalize();
    let cam_right = Vec3::new(cos_y, 0.0, sin_y).normalize();
    let cam_up = cam_right.cross(cam_fwd).normalize();

    // Base camera orientation
    let base_cam_rot = Mat3::from_cols(cam_right, cam_up, -cam_fwd);

    // Walking bobbing
    let bob_amp = if is_moving { 1.0 } else { 0.12 };
    let bob_x = (walk_phase * 0.5).sin() * 0.010 * bob_amp;
    let bob_y = (walk_phase).sin().abs() * 0.008 * bob_amp;

    // Shoulder anchor pinned to bottom-right of viewport with weapon sway offset
    let shoulder_pos = camera_pos
        + cam_right * (0.28 + bob_x + sway_offset.x)
        + cam_up * (-0.26 - bob_y + sway_offset.y)
        + cam_fwd * (0.14 + sway_offset.z);

    // Snappy swing interpolation (Minecraft style sqrt curve for snappy downward chop)
    let p = swing_progress.clamp(0.0, 1.0);
    let s_fast = (p.sqrt() * std::f32::consts::PI).sin();
    let s_full = (p * std::f32::consts::PI).sin();

    // Rotations around shoulder pivot
    let rest_pitch = 0.28;
    let rest_yaw = -0.18;
    let rest_roll = 0.12;

    let swing_pitch = rest_pitch - s_fast * 0.65;
    let swing_yaw = rest_yaw - s_full * 0.32;
    let swing_roll = rest_roll - s_fast * 0.38;

    let swing_mat = Mat3::from_rotation_y(swing_yaw)
        * Mat3::from_rotation_x(swing_pitch)
        * Mat3::from_rotation_z(swing_roll);
    let arm_rot = base_cam_rot * swing_mat;

    // Forward direction of the arm (-Z in local basis)
    let arm_fwd = arm_rot * Vec3::new(0.0, 0.0, -1.0);
    let punch_thrust = s_fast * 0.08;
    let arm_origin = shoulder_pos + arm_fwd * punch_thrust;

    // 1. Sleeve / base of arm
    let sleeve_center = arm_origin + arm_fwd * 0.055;
    push_box(
        &mut vertices,
        sleeve_center,
        Vec3::new(0.046, 0.046, 0.060),
        arm_rot,
        mono_color(shirt),
    );

    // 2. Rolled Cuff
    let cuff_center = arm_origin + arm_fwd * 0.115;
    push_box(
        &mut vertices,
        cuff_center,
        Vec3::new(0.049, 0.049, 0.015),
        arm_rot,
        mono_color(cuff_color),
    );

    // 3. Forearm (bare skin)
    let forearm_center = arm_origin + arm_fwd * 0.19;
    push_box(
        &mut vertices,
        forearm_center,
        Vec3::new(0.042, 0.042, 0.075),
        arm_rot,
        mono_color(skin),
    );

    // 4. Fist / Hand
    let fist_center = arm_origin + arm_fwd * 0.29;
    push_box(
        &mut vertices,
        fist_center,
        Vec3::new(0.044, 0.044, 0.038),
        arm_rot,
        mono_color(skin),
    );

    // 5. Held Block
    if let Some(block_colors) = held_block_colors {
        let block_pos = fist_center + arm_fwd * 0.06 + arm_rot * Vec3::new(0.0, 0.04, 0.0);
        let block_rot = arm_rot * Mat3::from_rotation_y(0.35) * Mat3::from_rotation_x(-0.25);
        if let Some(textures) = held_block_textures {
            push_textured_box(
                &mut vertices,
                block_pos,
                Vec3::splat(0.06),
                block_rot,
                block_colors,
                textures,
            );
        } else {
            push_box(
                &mut vertices,
                block_pos,
                Vec3::splat(0.06),
                block_rot,
                block_colors,
            );
        }
    }

    vertices
}
