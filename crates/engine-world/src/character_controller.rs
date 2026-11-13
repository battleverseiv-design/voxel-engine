use engine_core::{
    BlockId, BlockRegistry, LocomotionState, MaterialRegistry, MovementConfig, ShadingModel,
};
use glam::{IVec3, Vec3};
use crate::World;

/// Step sound effect type emitted during character movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepType {
    Footstep(BlockId),
    WaterSplash,
    WaterSwim,
}

/// Player input provided to the CharacterController on each simulation frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct ControllerInput {
    /// Normalized horizontal movement wish direction in world space (XZ).
    pub wish_dir: Vec3,
    /// Jump button held down.
    pub jump: bool,
    /// Jump button pressed on this specific frame.
    pub jump_pressed: bool,
    /// Sprint key held down.
    pub sprint: bool,
    /// Crouch key held down.
    pub crouch: bool,
    /// Flight mode toggle/active.
    pub fly: bool,
    /// Fly upward impulse.
    pub fly_up: bool,
    /// Fly downward impulse.
    pub fly_down: bool,
    /// Flight speed multiplier (e.g. when sprinting during flight).
    pub fly_speed_multiplier: f32,
    /// Camera yaw in radians for strafe roll calculations.
    pub look_yaw: f32,
}

/// Simulation result from the CharacterController.
#[derive(Debug, Clone, Copy)]
pub struct ControllerOutput {
    /// Updated feet position in world space.
    pub position: Vec3,
    /// Updated physical velocity vector.
    pub velocity: Vec3,
    /// Smoothed eye position in world space (ready for camera placement).
    pub eye_position: Vec3,
    /// Current locomotion state.
    pub state: LocomotionState,
    /// Whether feet are resting on solid ground.
    pub on_ground: bool,
    /// Whether character is submerged in water.
    pub in_water: bool,
    /// Camera roll tilt angle in radians from strafing.
    pub strafe_roll: f32,
    /// Procedural view bobbing offset (X = sway, Y = vertical bob).
    pub view_bob: Vec3,
    /// Audio event emitted on stride footstep.
    pub step_event: Option<StepType>,
}

/// Advanced physical character controller with AABB resolution, step-up mechanics,
/// sneak edge protection, and realistic locomotion physics.
#[derive(Debug, Clone)]
pub struct CharacterController {
    pub position: Vec3,
    pub velocity: Vec3,
    pub state: LocomotionState,
    pub config: MovementConfig,
    pub on_ground: bool,
    pub in_water: bool,
    pub is_crouching: bool,
    pub coyote_timer: f32,
    pub jump_buffer_timer: f32,
    pub smoothed_eye_y: f32,
    pub bob_phase: f32,
    pub step_distance_accumulated: f32,
    pub strafe_roll_angle: f32,
    pub current_height: f32,
}

impl CharacterController {
    pub fn new(spawn_pos: Vec3, config: MovementConfig) -> Self {
        let standing_h = config.standing_height;
        Self {
            position: spawn_pos,
            velocity: Vec3::ZERO,
            state: LocomotionState::Grounded,
            smoothed_eye_y: spawn_pos.y + standing_h * 0.90,
            current_height: standing_h,
            on_ground: false,
            in_water: false,
            is_crouching: false,
            coyote_timer: 0.0,
            jump_buffer_timer: 0.0,
            bob_phase: 0.0,
            step_distance_accumulated: 0.0,
            strafe_roll_angle: 0.0,
            config,
        }
    }

    /// Checks if standing up is blocked by overhead obstacles.
    pub fn can_stand_up(
        &self,
        world: &World,
        registry: &BlockRegistry,
        voxel_scale: f32,
    ) -> bool {
        let radius = self.config.collider_radius;
        let standing_h = self.config.standing_height;
        !check_aabb_collision(self.position, radius, standing_h, world, registry, voxel_scale)
    }

    /// Primary physics step executed on each simulation tick.
    pub fn step(
        &mut self,
        input: &ControllerInput,
        world: &World,
        registry: &BlockRegistry,
        materials: &MaterialRegistry,
        dt: f32,
        voxel_scale: f32,
    ) -> ControllerOutput {
        let dt = dt.clamp(0.0001, 0.05);
        let scale = if voxel_scale <= 0.0001 { 1.0 } else { voxel_scale };

        // 1. Water submergence detection (feet and torso)
        let feet_check = to_voxel_pos(self.position + Vec3::new(0.0, 0.15, 0.0), scale);
        let torso_check = to_voxel_pos(self.position + Vec3::new(0.0, self.current_height * 0.65, 0.0), scale);
        let feet_block = world.get_block(feet_check);
        let torso_block = world.get_block(torso_check);

        let feet_in_water = registry.get(feet_block).map(|d| d.shading_model == ShadingModel::Water).unwrap_or(false);
        let torso_in_water = registry.get(torso_block).map(|d| d.shading_model == ShadingModel::Water).unwrap_or(false);
        self.in_water = feet_in_water || torso_in_water;

        // 2. Crouch state resolution with overhead ceiling check
        if input.crouch {
            self.is_crouching = true;
            self.current_height = self.config.crouching_height;
        } else if self.is_crouching {
            if self.can_stand_up(world, registry, scale) {
                self.is_crouching = false;
                self.current_height = self.config.standing_height;
            } else {
                // Low ceiling forces crouch
                self.is_crouching = true;
                self.current_height = self.config.crouching_height;
            }
        } else {
            self.current_height = self.config.standing_height;
        }

        // 3. Jump buffer and coyote timers
        if input.jump_pressed {
            self.jump_buffer_timer = self.config.jump_buffer_sec;
        } else {
            self.jump_buffer_timer = (self.jump_buffer_timer - dt).max(0.0);
        }

        if self.on_ground {
            self.coyote_timer = self.config.coyote_time_sec;
        } else {
            self.coyote_timer = (self.coyote_timer - dt).max(0.0);
        }

        let mut step_event = None;

        // 4. Locomotion processing based on mode
        if input.fly {
            self.state = LocomotionState::FlyingNoclip;
            let mut fly_wish = input.wish_dir;
            if input.fly_up {
                fly_wish.y += 1.0;
            }
            if input.fly_down {
                fly_wish.y -= 1.0;
            }
            let fly_dir = fly_wish.normalize_or_zero();
            let fly_speed = self.config.fly_speed * input.fly_speed_multiplier.max(1.0);
            self.position += fly_dir * fly_speed * dt;
            self.velocity = Vec3::ZERO;
            self.on_ground = false;
        } else if self.in_water {
            self.state = LocomotionState::Swimming;
            // Water physics: fluid drag and buoyancy
            let swim_speed = self.config.crouch_speed * 1.35;
            let target_vel_xz = input.wish_dir * swim_speed;
            self.velocity.x += (target_vel_xz.x - self.velocity.x) * (6.0 * dt).min(1.0);
            self.velocity.z += (target_vel_xz.z - self.velocity.z) * (6.0 * dt).min(1.0);

            // Water vertical motion
            if input.jump {
                self.velocity.y = (self.velocity.y + 12.0 * dt).min(3.2);
            } else if input.crouch {
                self.velocity.y = (self.velocity.y - 12.0 * dt).max(-3.2);
            } else {
                // Neutral buoyancy with gentle damping
                self.velocity.y *= (1.0 - 4.0 * dt).max(0.0);
            }

            let (new_pos, landed) = self.move_and_slide(
                world,
                registry,
                self.velocity * dt,
                scale,
                false,
            );
            self.position = new_pos;
            self.on_ground = landed;
            if landed && self.velocity.y < 0.0 {
                self.velocity.y = 0.0;
            }
        } else {
            // Standard Grounded / Airborne locomotion
            let wish_speed = if self.is_crouching {
                self.config.crouch_speed
            } else if input.sprint && input.wish_dir.length_squared() > 0.01 {
                self.config.sprint_speed
            } else {
                self.config.walk_speed
            };

            // Ground friction with surface material support
            if self.on_ground {
                let ground_voxel = to_voxel_pos(self.position - Vec3::new(0.0, 0.15, 0.0), scale);
                let surface_block = world.get_block(ground_voxel);
                let friction_multiplier = if let Some(def) = registry.get(surface_block) {
                    let top_mat_id = def.face_materials[0];
                    if let Some(mat) = materials.get(top_mat_id) {
                        // Dense solid surfaces have normal friction; slick surfaces reduce friction
                        (1.0 - mat.roughness * 0.15).clamp(0.4, 1.2)
                    } else {
                        (1.0 - def.roughness * 0.15).clamp(0.4, 1.2)
                    }
                } else {
                    1.0
                };

                let friction = self.config.ground_friction * friction_multiplier;
                let speed = self.velocity.length();
                if speed > 0.001 {
                    let drop = speed * friction * dt;
                    let new_speed = (speed - drop).max(0.0);
                    self.velocity *= new_speed / speed;
                }
            }

            // Vector acceleration
            let accel = if self.on_ground {
                self.config.ground_acceleration
            } else {
                self.config.air_acceleration
            };

            if input.wish_dir.length_squared() > 0.01 {
                let current_speed = self.velocity.x * input.wish_dir.x + self.velocity.z * input.wish_dir.z;
                let add_speed = wish_speed - current_speed;
                if add_speed > 0.0 {
                    let accel_speed = (accel * dt * wish_speed).min(add_speed);
                    self.velocity.x += input.wish_dir.x * accel_speed;
                    self.velocity.z += input.wish_dir.z * accel_speed;
                }
            }

            // Gravity
            self.velocity.y -= self.config.gravity * dt;

            // Jump check with coyote time and jump buffer
            let can_jump = (self.on_ground || self.coyote_timer > 0.0) && self.jump_buffer_timer > 0.0;
            if can_jump {
                self.velocity.y = self.config.jump_impulse;
                self.on_ground = false;
                self.coyote_timer = 0.0;
                self.jump_buffer_timer = 0.0;
            }

            // Collision resolution with step-up and sneak edge guard
            let sneak_guard = self.is_crouching && self.on_ground;
            let (new_pos, landed) = self.move_and_slide_with_step(
                world,
                registry,
                self.velocity * dt,
                scale,
                sneak_guard,
            );

            if landed && self.velocity.y < 0.0 {
                self.velocity.y = 0.0;
            } else if new_pos.y == self.position.y && self.velocity.y > 0.0 {
                self.velocity.y = 0.0;
            }

            self.position = new_pos;
            self.on_ground = landed;

            // Update state
            if !self.on_ground {
                self.state = LocomotionState::Airborne;
            } else if self.is_crouching {
                self.state = LocomotionState::Crouching;
            } else if input.sprint && input.wish_dir.length_squared() > 0.01 {
                self.state = LocomotionState::Sprinting;
            } else {
                self.state = LocomotionState::Grounded;
            }
        }

        // 5. Stride accumulation and footstep audio triggering
        let horiz_speed = Vec3::new(self.velocity.x, 0.0, self.velocity.z).length();
        if horiz_speed > 0.15 && (self.on_ground || self.in_water) {
            let stride_distance = if self.is_crouching {
                1.10
            } else if self.state == LocomotionState::Sprinting {
                2.35
            } else {
                1.75
            };

            self.step_distance_accumulated += horiz_speed * dt;
            if self.step_distance_accumulated >= stride_distance {
                self.step_distance_accumulated = 0.0;
                if self.in_water {
                    step_event = Some(StepType::WaterSwim);
                } else {
                    let ground_voxel = to_voxel_pos(self.position - Vec3::new(0.0, 0.15, 0.0), scale);
                    let mut b = world.get_block(ground_voxel);
                    if b == BlockId::AIR {
                        b = world.get_block(to_voxel_pos(self.position - Vec3::new(0.0, 0.50, 0.0), scale));
                    }
                    step_event = Some(StepType::Footstep(b));
                }
            }
        } else {
            self.step_distance_accumulated = (self.step_distance_accumulated - dt * 0.5).max(0.0);
        }

        // 6. Camera smoothing, view bobbing and strafe roll
        let target_eye_offset = if self.is_crouching {
            self.config.crouching_height * 0.88
        } else {
            self.config.standing_height * 0.90
        };
        let target_eye_y = self.position.y + target_eye_offset;

        // Exponential smoothing prevents camera snapping on steps
        let eye_lerp_rate = 18.0;
        let eye_alpha = 1.0 - (-eye_lerp_rate * dt).exp();
        self.smoothed_eye_y += (target_eye_y - self.smoothed_eye_y) * eye_alpha;

        // Procedural View Bobbing
        let bob_amp = if self.on_ground && horiz_speed > 0.15 {
            (horiz_speed / self.config.sprint_speed).min(1.0)
        } else {
            0.0
        };
        self.bob_phase += horiz_speed * dt * 5.2;
        let view_bob = Vec3::new(
            (self.bob_phase * 0.5).sin() * 0.028 * bob_amp,
            (self.bob_phase).sin().abs() * 0.042 * bob_amp,
            0.0,
        );

        // Strafe Roll Angle (camera tilts into sideways movements)
        let (sin_y, cos_y) = input.look_yaw.sin_cos();
        let right_dir = Vec3::new(cos_y, 0.0, sin_y).normalize_or_zero();
        let strafe_dot = self.velocity.dot(right_dir);
        let target_roll = (-strafe_dot * 0.0055).clamp(-0.045, 0.045);
        let roll_alpha = 1.0 - (-10.0 * dt).exp();
        self.strafe_roll_angle += (target_roll - self.strafe_roll_angle) * roll_alpha;

        ControllerOutput {
            position: self.position,
            velocity: self.velocity,
            eye_position: Vec3::new(self.position.x, self.smoothed_eye_y, self.position.z) + view_bob,
            state: self.state,
            on_ground: self.on_ground,
            in_water: self.in_water,
            strafe_roll: self.strafe_roll_angle,
            view_bob,
            step_event,
        }
    }

    /// Moves character with AABB collision resolution.
    fn move_and_slide(
        &self,
        world: &World,
        registry: &BlockRegistry,
        delta: Vec3,
        scale: f32,
        sneak_guard: bool,
    ) -> (Vec3, bool) {
        let mut pos = self.position;
        let r = self.config.collider_radius;
        let h = self.current_height;

        // X axis movement
        let next_x = Vec3::new(pos.x + delta.x, pos.y, pos.z);
        if !check_aabb_collision(next_x, r, h, world, registry, scale) {
            if !sneak_guard || has_ground_support(next_x, r, world, registry, scale) {
                pos.x = next_x.x;
            }
        }

        // Z axis movement
        let next_z = Vec3::new(pos.x, pos.y, pos.z + delta.z);
        if !check_aabb_collision(next_z, r, h, world, registry, scale) {
            if !sneak_guard || has_ground_support(next_z, r, world, registry, scale) {
                pos.z = next_z.z;
            }
        }

        // Y axis movement
        let mut landed = false;
        let next_y = Vec3::new(pos.x, pos.y + delta.y, pos.z);
        if !check_aabb_collision(next_y, r, h, world, registry, scale) {
            pos.y = next_y.y;
        } else if delta.y < 0.0 {
            landed = true;
        }

        (pos, landed)
    }

    /// Moves character with automatic step-up over low obstacles.
    fn move_and_slide_with_step(
        &self,
        world: &World,
        registry: &BlockRegistry,
        delta: Vec3,
        scale: f32,
        sneak_guard: bool,
    ) -> (Vec3, bool) {
        let mut pos = self.position;
        let r = self.config.collider_radius;
        let h = self.current_height;
        let step_h = self.config.step_height;

        // Try direct horizontal movement
        let next_xz = Vec3::new(pos.x + delta.x, pos.y, pos.z + delta.z);
        let blocked = check_aabb_collision(next_xz, r, h, world, registry, scale);

        if !blocked {
            if !sneak_guard || has_ground_support(next_xz, r, world, registry, scale) {
                pos = next_xz;
            }
        } else if self.on_ground && (delta.x.abs() > 0.0001 || delta.z.abs() > 0.0001) {
            // Attempt Step-Up: lift character by step_h, step forward, then step down
            let step_lift = Vec3::new(pos.x, pos.y + step_h, pos.z);
            if !check_aabb_collision(step_lift, r, h, world, registry, scale) {
                let step_forward = step_lift + Vec3::new(delta.x, 0.0, delta.z);
                if !check_aabb_collision(step_forward, r, h, world, registry, scale) {
                    // Find floor beneath step_forward
                    let mut found_floor = false;
                    let mut floor_y = pos.y;
                    let samples = 12;
                    for s in 0..=samples {
                        let test_y = step_lift.y - (step_h * (s as f32 / samples as f32));
                        let test_pos = Vec3::new(step_forward.x, test_y, step_forward.z);
                        if check_aabb_collision(test_pos, r, h, world, registry, scale) {
                            floor_y = test_y + 0.01;
                            found_floor = true;
                            break;
                        }
                    }
                    if found_floor && floor_y >= pos.y {
                        pos = Vec3::new(step_forward.x, floor_y, step_forward.z);
                    }
                }
            }

            // Fallback: individual axis test if full step-up was blocked
            if pos.x == self.position.x && pos.z == self.position.z {
                let next_x = Vec3::new(pos.x + delta.x, pos.y, pos.z);
                if !check_aabb_collision(next_x, r, h, world, registry, scale) {
                    if !sneak_guard || has_ground_support(next_x, r, world, registry, scale) {
                        pos.x = next_x.x;
                    }
                }
                let next_z = Vec3::new(pos.x, pos.y, pos.z + delta.z);
                if !check_aabb_collision(next_z, r, h, world, registry, scale) {
                    if !sneak_guard || has_ground_support(next_z, r, world, registry, scale) {
                        pos.z = next_z.z;
                    }
                }
            }
        }

        // Vertical movement
        let mut landed = false;
        let next_y = Vec3::new(pos.x, pos.y + delta.y, pos.z);
        if !check_aabb_collision(next_y, r, h, world, registry, scale) {
            pos.y = next_y.y;
        } else if delta.y < 0.0 {
            landed = true;
        }

        (pos, landed)
    }
}

/// Converts a world coordinate into integer voxel coordinates using div_euclid logic.
#[inline]
pub fn to_voxel_pos(pos: Vec3, voxel_scale: f32) -> IVec3 {
    let scale = if voxel_scale <= 0.0001 { 1.0 } else { voxel_scale };
    let sx = (pos.x / scale).floor() as i64;
    let sy = (pos.y / scale).floor() as i64;
    let sz = (pos.z / scale).floor() as i64;

    IVec3::new(
        sx.div_euclid(1) as i32,
        sy.div_euclid(1) as i32,
        sz.div_euclid(1) as i32,
    )
}

/// Checks if an AABB collides with any solid voxel in the world.
#[inline]
pub fn check_aabb_collision(
    feet: Vec3,
    radius: f32,
    height: f32,
    world: &World,
    registry: &BlockRegistry,
    voxel_scale: f32,
) -> bool {
    let scale = if voxel_scale <= 0.0001 { 1.0 } else { voxel_scale };

    let min_vx = ((feet.x - radius + 0.001) / scale).floor() as i32;
    let max_vx = ((feet.x + radius - 0.001) / scale).floor() as i32;
    let min_vy = ((feet.y + 0.001) / scale).floor() as i32;
    let max_vy = ((feet.y + height - 0.001) / scale).floor() as i32;
    let min_vz = ((feet.z - radius + 0.001) / scale).floor() as i32;
    let max_vz = ((feet.z + radius - 0.001) / scale).floor() as i32;

    for y in min_vy..=max_vy {
        for z in min_vz..=max_vz {
            for x in min_vx..=max_vx {
                let block = world.get_block(IVec3::new(x, y, z));
                if registry.is_solid(block) {
                    return true;
                }
            }
        }
    }
    false
}

/// Checks if there is solid ground supporting the character's feet (for Sneak Edge Guard).
#[inline]
fn has_ground_support(
    feet: Vec3,
    radius: f32,
    world: &World,
    registry: &BlockRegistry,
    scale: f32,
) -> bool {
    let check_y = feet.y - 0.18;
    let offsets = [
        Vec3::ZERO,
        Vec3::new(radius * 0.75, 0.0, 0.0),
        Vec3::new(-radius * 0.75, 0.0, 0.0),
        Vec3::new(0.0, 0.0, radius * 0.75),
        Vec3::new(0.0, 0.0, -radius * 0.75),
    ];

    for off in offsets {
        let p = Vec3::new(feet.x + off.x, check_y, feet.z + off.z);
        let vpos = to_voxel_pos(p, scale);
        if registry.is_solid(world.get_block(vpos)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::World;
    use engine_core::{BlockDef, BlockRegistry, MaterialRegistry};

    fn make_test_registry() -> BlockRegistry {
        let mut reg = BlockRegistry::new();
        reg.register(BlockDef::solid_tinted(
            "Stone",
            [0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
        ));
        reg
    }

    #[test]
    fn test_voxel_coord_conversion() {
        let p = Vec3::new(10.5, 2.1, -3.2);
        let v = to_voxel_pos(p, 1.0);
        assert_eq!(v, IVec3::new(10, 2, -4));
    }

    #[test]
    fn test_sneak_guard_prevents_falling() {
        let reg = make_test_registry();
        let materials = MaterialRegistry::new();
        let mut world = World::new();
        world.set_block(IVec3::new(0, 0, 0), BlockId(1));

        let mut controller = CharacterController::new(Vec3::new(0.5, 1.0, 0.5), MovementConfig::default());
        controller.on_ground = true;

        let input = ControllerInput {
            wish_dir: Vec3::X,
            crouch: true,
            ..Default::default()
        };

        let out = controller.step(&input, &world, &reg, &materials, 0.05, 1.0);
        assert!(out.position.x < 1.0);
    }

    #[test]
    fn test_coyote_time_allows_delayed_jump() {
        let reg = make_test_registry();
        let materials = MaterialRegistry::new();
        let world = World::new();

        let mut controller = CharacterController::new(Vec3::new(0.0, 10.0, 0.0), MovementConfig::default());
        controller.coyote_timer = 0.10;
        controller.on_ground = false;

        let input = ControllerInput {
            jump_pressed: true,
            ..Default::default()
        };

        let out = controller.step(&input, &world, &reg, &materials, 0.016, 1.0);
        assert!(out.velocity.y > 5.0);
    }
}
