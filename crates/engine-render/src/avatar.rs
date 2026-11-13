use engine_core::AvatarModel;
use engine_world::VoxelVertex;
use glam::{Mat3, Vec3};

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

/// Builds dynamic low-poly avatar mesh with procedural stance and speed blending.
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
    held_block_colors: Option<[[f32; 3]; 6]>,
) -> Vec<VoxelVertex> {
    let mut vertices = Vec::with_capacity(300);

    let (skin, shirt, pants, hair) = match avatar {
        AvatarModel::None => return vertices,
        AvatarModel::SimpleHumanoid {
            skin_color,
            shirt_color,
            pants_color,
            hair_color,
        } => (*skin_color, *shirt_color, *pants_color, *hair_color),
    };

    let mono_color = |col: [f32; 3]| [col; 6];

    // Engine orientation: forward = (sin_yaw, 0, -cos_yaw)
    let (sin_y, cos_y) = yaw.sin_cos();
    let body_forward = Vec3::new(sin_y, 0.0, -cos_y).normalize();
    let body_right = Vec3::new(cos_y, 0.0, sin_y).normalize();
    let body_up = Vec3::Y;

    // Body rotation matrix with stance adaptations
    let mut body_rot = Mat3::from_cols(body_right, body_up, -body_forward);

    if is_swimming {
        // Horizontal swimming posture
        body_rot = body_rot * Mat3::from_rotation_x(-1.25);
    } else if is_crouching {
        // Forward torso tilt when crouching
        body_rot = body_rot * Mat3::from_rotation_x(0.32);
    }

    // Speed-based swing amplitude blending
    let speed_ratio = (horizontal_speed / 5.2).clamp(0.0, 1.5);
    let swing_amp = if is_moving {
        0.58 * speed_ratio.max(0.35)
    } else {
        0.0
    };

    let (leg_swing, left_arm_swing, right_arm_swing) = if is_swimming {
        let kick = (walk_phase * 1.5).sin() * 0.45;
        (kick, -kick * 0.8, kick * 0.8)
    } else if is_in_air {
        // Airborne jump / fall pose
        (0.22, -0.45, -0.45)
    } else {
        let leg = (walk_phase).sin() * swing_amp;
        (leg, -leg, leg)
    };

    // Stance height adjustments
    let height_drop = if is_crouching { 0.38 } else { 0.0 };
    let base_feet = feet_pos - body_up * height_drop;

    // 1. Torso (0.44 wide, 0.62 tall, 0.24 deep)
    let torso_center = base_feet + body_up * 1.02;
    push_box(
        &mut vertices,
        torso_center,
        Vec3::new(0.22, 0.31, 0.12),
        body_rot,
        mono_color(shirt),
    );

    // 2. Left Leg (pivot at hip y = 0.71)
    let left_hip = base_feet + body_up * 0.71 - body_right * 0.11;
    let crouch_leg_angle = if is_crouching { -0.52 } else { 0.0 };
    let left_leg_rot = body_rot * Mat3::from_rotation_x(leg_swing + crouch_leg_angle);
    let left_leg_center = left_hip + left_leg_rot * Vec3::new(0.0, -0.355, 0.0);
    push_box(
        &mut vertices,
        left_leg_center,
        Vec3::new(0.10, 0.355, 0.10),
        left_leg_rot,
        mono_color(pants),
    );

    // 3. Right Leg (pivot at hip y = 0.71)
    let right_hip = base_feet + body_up * 0.71 + body_right * 0.11;
    let right_leg_rot = body_rot * Mat3::from_rotation_x(-leg_swing + crouch_leg_angle);
    let right_leg_center = right_hip + right_leg_rot * Vec3::new(0.0, -0.355, 0.0);
    push_box(
        &mut vertices,
        right_leg_center,
        Vec3::new(0.10, 0.355, 0.10),
        right_leg_rot,
        mono_color(pants),
    );

    // 4. Left Arm (pivot at shoulder y = 1.25)
    let left_shoulder = base_feet + body_up * 1.25 - body_right * 0.32;
    let left_air_spread = if is_in_air { 0.30 } else { 0.0 };
    let left_arm_rot = body_rot
        * Mat3::from_rotation_z(left_air_spread)
        * Mat3::from_rotation_x(left_arm_swing);
    let left_arm_center = left_shoulder + left_arm_rot * Vec3::new(0.0, -0.28, 0.0);
    push_box(
        &mut vertices,
        left_arm_center,
        Vec3::new(0.09, 0.28, 0.09),
        left_arm_rot,
        mono_color(skin),
    );

    // 5. Right Arm (pivot at shoulder y = 1.25)
    let right_shoulder = base_feet + body_up * 1.25 + body_right * 0.32;
    let right_air_spread = if is_in_air { -0.30 } else { 0.0 };
    let right_arm_angle = if held_block_colors.is_some() {
        0.55 - right_arm_swing * 0.25
    } else {
        right_arm_swing
    };
    let right_arm_rot = body_rot
        * Mat3::from_rotation_z(right_air_spread)
        * Mat3::from_rotation_x(right_arm_angle);
    let right_arm_center = right_shoulder + right_arm_rot * Vec3::new(0.0, -0.28, 0.0);
    push_box(
        &mut vertices,
        right_arm_center,
        Vec3::new(0.09, 0.28, 0.09),
        right_arm_rot,
        mono_color(skin),
    );

    // 6. Held Block in Right Hand (miniature cube held forward in hand)
    if let Some(block_colors) = held_block_colors {
        let hand_pos = right_shoulder + right_arm_rot * Vec3::new(0.0, -0.58, -0.12);
        push_box(
            &mut vertices,
            hand_pos,
            Vec3::splat(0.09),
            right_arm_rot,
            block_colors,
        );
    }

    // 7. Head (pivot at neck y = 1.34)
    let neck_pos = base_feet + body_up * 1.34;
    let head_rot = body_rot * Mat3::from_rotation_x(pitch);
    let head_center = neck_pos + head_rot * Vec3::new(0.0, 0.21, 0.0);
    push_box(
        &mut vertices,
        head_center,
        Vec3::new(0.18, 0.18, 0.18),
        head_rot,
        mono_color(skin),
    );

    // 8. Eyes on the front face (-Z direction)
    let eye_white = [0.94, 0.94, 0.96];
    let eye_pupil = [0.15, 0.18, 0.25];
    let left_eye_center = head_center + head_rot * Vec3::new(-0.065, 0.015, -0.182);
    let right_eye_center = head_center + head_rot * Vec3::new(0.065, 0.015, -0.182);
    push_box(
        &mut vertices,
        left_eye_center,
        Vec3::new(0.038, 0.032, 0.005),
        head_rot,
        mono_color(eye_white),
    );
    push_box(
        &mut vertices,
        left_eye_center + head_rot * Vec3::new(-0.012, 0.0, -0.002),
        Vec3::new(0.022, 0.026, 0.005),
        head_rot,
        mono_color(eye_pupil),
    );
    push_box(
        &mut vertices,
        right_eye_center,
        Vec3::new(0.038, 0.032, 0.005),
        head_rot,
        mono_color(eye_white),
    );
    push_box(
        &mut vertices,
        right_eye_center + head_rot * Vec3::new(0.012, 0.0, -0.002),
        Vec3::new(0.022, 0.026, 0.005),
        head_rot,
        mono_color(eye_pupil),
    );

    // 9. Hair cap (styled over top and back of head)
    let hair_center = head_center + head_rot * Vec3::new(0.0, 0.07, 0.025);
    push_box(
        &mut vertices,
        hair_center,
        Vec3::new(0.186, 0.11, 0.186),
        head_rot,
        mono_color(hair),
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
) -> Vec<VoxelVertex> {
    let mut vertices = Vec::with_capacity(120);

    let (skin, shirt) = match avatar {
        AvatarModel::None => return vertices,
        AvatarModel::SimpleHumanoid {
            skin_color,
            shirt_color,
            ..
        } => (*skin_color, *shirt_color),
    };

    let mono_color = |col: [f32; 3]| [col; 6];

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

    // 1. Sleeve / base of arm (stays pinned near shoulder)
    let sleeve_center = arm_origin + arm_fwd * 0.06;
    push_box(
        &mut vertices,
        sleeve_center,
        Vec3::new(0.046, 0.046, 0.065),
        arm_rot,
        mono_color(shirt),
    );

    // 2. Forearm (extends forward from sleeve)
    let forearm_center = arm_origin + arm_fwd * 0.17;
    push_box(
        &mut vertices,
        forearm_center,
        Vec3::new(0.042, 0.042, 0.11),
        arm_rot,
        mono_color(skin),
    );

    // 3. Fist (hand at the end of the arm)
    let fist_center = arm_origin + arm_fwd * 0.29;
    push_box(
        &mut vertices,
        fist_center,
        Vec3::new(0.045, 0.045, 0.035),
        arm_rot,
        mono_color(skin),
    );

    // 4. Held Block (miniature cube held in hand)
    if let Some(block_colors) = held_block_colors {
        let block_pos = fist_center + arm_fwd * 0.06 + arm_rot * Vec3::new(0.0, 0.04, 0.0);
        let block_rot = arm_rot * Mat3::from_rotation_y(0.35) * Mat3::from_rotation_x(-0.25);
        push_box(
            &mut vertices,
            block_pos,
            Vec3::splat(0.06),
            block_rot,
            block_colors,
        );
    }

    vertices
}
