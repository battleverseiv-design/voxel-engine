mod audio;
mod settings;
mod world_manager;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use engine_core::{
    AvatarModel, BlockId, BlockRegistry, CommandHost, CommandRegistry, EnvironmentProvider,
    GameMode, Inventory, ItemStack, MaterialRegistry, MovementConfig,
    PlayerMode, ReferenceFrameId, RelativeTransform, ServerHostProfile, WorldGenerator,
};
use engine_net::{
    ClientEvent, DiscoveryBeacon, DiscoveryScanner, GameClient, HostEvent,
    Packet, RemotePlayer, ServerHost, DEFAULT_PORT,
};
use engine_render::{
    build_avatar_mesh, build_first_person_arm_mesh, push_textured_box, Camera, RenderError, Renderer,
};
use engine_world::{
    world_to_chunk, CharacterController, ControllerInput, StepType,
    VoxelContraption, World, WorldStreamer, CHUNK_SIZE_I32,
};
use game_survival::{LithosphereWorldGen, SurvivalMode};
use glam::{IVec3, Mat3, Vec3};
use settings::{keycode_to_name, name_to_keycode, Settings};
use world_manager::{WorldManager, WorldMeta};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const PLAYER_RADIUS: f32 = 0.32;
const PLAYER_HEIGHT: f32 = 1.75;
const PLAYER_EYE_HEIGHT: f32 = 1.55;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    /// Pure offline singleplayer. Simulation fully freezes on Pause (ESC).
    Solo,
    /// Player hosts a co-op server. Simulation never freezes on Pause (ESC); admin panel active.
    Host,
    /// Player joined a remote host server.
    Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostConfigTab {
    Network,
    Sandbox,
    World,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinTab {
    Lan,
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuScreen {
    Title,
    SelectWorld,
    CreateWorld,
    HostServer,
    JoinServer,
    Playing,
    Pause,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CameraPerspective {
    #[default]
    FirstPerson,
    ThirdPersonBack,
    ThirdPersonFront,
}

impl CameraPerspective {
    fn label_ru(self) -> &'static str {
        match self {
            Self::FirstPerson => "1-е лицо",
            Self::ThirdPersonBack => "3-е лицо (вид сзади)",
            Self::ThirdPersonFront => "3-е лицо (вид спереди / лицо)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebindTarget {
    Forward,
    Backward,
    Left,
    Right,
    JumpUp,
    Down,
    Sprint,
    ToggleFly,
    Chat,
    Command,
}

#[derive(Debug, Clone)]
struct ChatEntry {
    text: String,
    color: egui::Color32,
    timestamp: Instant,
}

struct InputState {
    keys: HashSet<KeyCode>,
    cursor_locked: bool,
    mouse_delta: (f32, f32),
}

impl InputState {
    fn new() -> Self {
        Self {
            keys: HashSet::new(),
            cursor_locked: false,
            mouse_delta: (0.0, 0.0),
        }
    }

    fn is_bound_down(&self, key_name: &str) -> bool {
        name_to_keycode(key_name).is_some_and(|code| self.keys.contains(&code))
    }
}

struct EngineApp {
    mode: Box<dyn GameMode>,
    registry: Arc<BlockRegistry>,
    materials: Arc<MaterialRegistry>,
    commands: Arc<CommandRegistry>,
    generator: Arc<dyn WorldGenerator>,
    world: World,
    streamer: WorldStreamer,
    camera: Camera,
    target_yaw: f32,
    target_pitch: f32,
    velocity: Vec3,
    on_ground: bool,
    controller: CharacterController,
    jump_just_pressed: bool,
    hand_sway: Vec3,
    player_mode: PlayerMode,
    fly_mode: bool,
    allow_fly: bool,
    last_space_press: Option<Instant>,
    inventory: Inventory,
    break_target: Option<IVec3>,
    break_progress: f32,
    break_time: f32,
    inventory_open: bool,
    held_stack: Option<ItemStack>,
    settings: Settings,
    screen: MenuScreen,
    current_world: Option<WorldMeta>,
    cached_world_list: Vec<WorldMeta>,
    selected_world_id: Option<String>,
    create_world_name: String,
    create_world_seed: String,
    create_world_mode: String,
    create_world_gen: String,
    chat_open: bool,
    chat_input: String,
    chat_focus_requested: bool,
    chat_history: Vec<ChatEntry>,
    rebinding: Option<RebindTarget>,
    input: InputState,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    last_frame: Instant,
    fps_timer: Instant,
    frame_counter: u32,
    displayed_fps: u32,
    time_of_day: f32,
    time_speed: f32,
    environment: Arc<dyn EnvironmentProvider>,
    avatar: AvatarModel,
    perspective: CameraPerspective,
    walk_phase: f32,
    arm_swing_progress: f32,
    arm_swing_active: bool,
    lmb_held: bool,
    server_host: Option<ServerHost>,
    game_client: Option<GameClient>,
    host_remote_players: HashMap<u32, RemotePlayer>,
    discovery_beacon: Option<DiscoveryBeacon>,
    discovery_scanner: DiscoveryScanner,
    net_status: Option<(String, egui::Color32, Instant)>,
    connect_ip_input: String,
    server_port_input: String,
    player_name_input: String,
    last_net_sync: Instant,
    panorama_angle: f32,
    player_pos: Vec3,
    player_yaw: f32,
    player_pitch: f32,
    return_screen: MenuScreen,
    health: f32,
    last_damage_time: Option<Instant>,
    last_attack_time: Option<Instant>,
    creative_search: String,
    last_attacker_name: Option<String>,
    last_damage_info: Option<(String, f32, Instant)>,
    last_hitmarker: Option<(Instant, bool, f32)>,
    camera_flinch: f32,
    death_fall_progress: f32,
    last_regen_sync: Instant,
    // ===== [TEMPORARY PHYSICS TEST: START] =====
    test_physics_blocks: Vec<VoxelContraption>,
    // ===== [TEMPORARY PHYSICS TEST: END] =====
    session_type: SessionType,
    server_profiles: Vec<(String, ServerHostProfile)>,
    selected_server_profile_idx: usize,
    active_host_profile: ServerHostProfile,
    host_profile_name_input: String,
    host_active_tab: HostConfigTab,
    host_status_msg: Option<(String, bool)>,
    join_tab: JoinTab,
    join_password_input: String,
    dropped_items: Vec<DroppedItem>,
    next_dropped_item_id: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct DroppedItem {
    pub id: u64,
    pub position: Vec3,
    pub velocity: Vec3,
    pub block_id: BlockId,
    pub count: u32,
    pub age: f32,
    pub pickup_delay: f32,
}

#[derive(Debug, Clone, Copy)]
pub enum SlotAction {
    LeftClick(usize),
    RightClick(usize),
    ShiftClick(usize),
}

fn render_block_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    size: f32,
    block_id: BlockId,
    registry: &BlockRegistry,
) {
    let def = registry.get(block_id);
    let top_col = def
        .map(|d| {
            let c = d.face_colors[0];
            egui::Color32::from_rgb(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            )
        })
        .unwrap_or(egui::Color32::GRAY);

    let left_col = def
        .map(|d| {
            let c = d.face_colors[4];
            egui::Color32::from_rgb(
                (c[0] * 205.0) as u8,
                (c[1] * 205.0) as u8,
                (c[2] * 205.0) as u8,
            )
        })
        .unwrap_or(egui::Color32::DARK_GRAY);

    let right_col = def
        .map(|d| {
            let c = d.face_colors[3];
            egui::Color32::from_rgb(
                (c[0] * 160.0) as u8,
                (c[1] * 160.0) as u8,
                (c[2] * 160.0) as u8,
            )
        })
        .unwrap_or(egui::Color32::BLACK);

    let rx = size * 0.50;
    let ry = size * 0.28;
    let h = size * 0.48;

    let p_top = center - egui::vec2(0.0, ry + h * 0.45);
    let p_top_right = center + egui::vec2(rx, -h * 0.45);
    let p_top_left = center + egui::vec2(-rx, -h * 0.45);
    let p_mid = center + egui::vec2(0.0, ry - h * 0.45);
    let p_bot = center + egui::vec2(0.0, ry + h * 0.55);
    let p_bot_right = center + egui::vec2(rx, h * 0.55);
    let p_bot_left = center + egui::vec2(-rx, h * 0.55);

    // 1. Top face (isometric rhombus)
    painter.add(egui::epaint::Shape::convex_polygon(
        vec![p_top, p_top_right, p_mid, p_top_left],
        top_col,
        egui::Stroke::new(0.5_f32, egui::Color32::from_black_alpha(75)),
    ));

    // 2. Left face (isometric parallelogram)
    painter.add(egui::epaint::Shape::convex_polygon(
        vec![p_top_left, p_mid, p_bot, p_bot_left],
        left_col,
        egui::Stroke::new(0.5_f32, egui::Color32::from_black_alpha(75)),
    ));

    // 3. Right face (isometric parallelogram)
    painter.add(egui::epaint::Shape::convex_polygon(
        vec![p_mid, p_top_right, p_bot_right, p_bot],
        right_col,
        egui::Stroke::new(0.5_f32, egui::Color32::from_black_alpha(75)),
    ));
}

fn render_inventory_slot(
    ui: &mut egui::Ui,
    slot_idx: usize,
    slot: Option<ItemStack>,
    is_active: bool,
    registry: &BlockRegistry,
    action: &mut Option<SlotAction>,
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::click());
    if response.clicked() {
        let shift = ui.input(|i| i.modifiers.shift);
        if shift {
            *action = Some(SlotAction::ShiftClick(slot_idx));
        } else {
            *action = Some(SlotAction::LeftClick(slot_idx));
        }
    } else if response.secondary_clicked() {
        *action = Some(SlotAction::RightClick(slot_idx));
    }

    let is_hovered = response.hovered();
    let bg_color = if is_active {
        egui::Color32::from_rgb(52, 78, 115)
    } else if is_hovered {
        egui::Color32::from_rgb(46, 54, 68)
    } else {
        egui::Color32::from_rgb(24, 28, 36)
    };

    let border_stroke = if is_active {
        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 225, 120))
    } else if is_hovered {
        egui::Stroke::new(1.8_f32, egui::Color32::from_rgb(150, 185, 235))
    } else {
        egui::Stroke::new(1.2_f32, egui::Color32::from_rgb(55, 65, 80))
    };

    ui.painter().rect_filled(rect, 5.0, bg_color);
    ui.painter().rect_stroke(rect, 5.0, border_stroke, egui::StrokeKind::Inside);

    if let Some(stack) = slot {
        render_block_icon(
            ui.painter(),
            rect.center() - egui::vec2(0.0, 1.0),
            22.0,
            stack.block_id,
            registry,
        );

        let count_text = format!("{}", stack.count);
        let text_pos = rect.max - egui::vec2(4.0, 3.0);
        ui.painter().text(
            text_pos + egui::vec2(1.0, 1.0),
            egui::Align2::RIGHT_BOTTOM,
            &count_text,
            egui::FontId::proportional(12.0),
            egui::Color32::BLACK,
        );
        ui.painter().text(
            text_pos,
            egui::Align2::RIGHT_BOTTOM,
            &count_text,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );

        if let Some(def) = registry.get(stack.block_id) {
            let name = def.name.split(':').last().unwrap_or(&def.name);
            response.on_hover_ui(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).strong().color(egui::Color32::WHITE));
                    ui.label(
                        egui::RichText::new(format!("x{}", stack.count))
                            .color(egui::Color32::from_rgb(255, 215, 120)),
                    );
                });
            });
        }
    }
}

fn render_hud_hotbar_slot(
    ui: &mut egui::Ui,
    slot_idx: usize,
    slot: Option<ItemStack>,
    is_active: bool,
    registry: &BlockRegistry,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());

    let bg_color = if is_active {
        egui::Color32::from_rgba_unmultiplied(65, 95, 140, 215)
    } else {
        egui::Color32::from_rgba_unmultiplied(20, 24, 32, 195)
    };

    let border_stroke = if is_active {
        egui::Stroke::new(2.5_f32, egui::Color32::from_rgb(255, 230, 140))
    } else {
        egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(45))
    };

    ui.painter().rect_filled(rect, 6.0, bg_color);
    ui.painter().rect_stroke(rect, 6.0, border_stroke, egui::StrokeKind::Inside);

    ui.painter().text(
        rect.min + egui::vec2(4.0, 3.0),
        egui::Align2::LEFT_TOP,
        format!("{}", slot_idx + 1),
        egui::FontId::proportional(10.0),
        if is_active {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_white_alpha(120)
        },
    );

    if let Some(stack) = slot {
        render_block_icon(
            ui.painter(),
            rect.center() + egui::vec2(0.0, 1.0),
            22.0,
            stack.block_id,
            registry,
        );

        if stack.count > 1 {
            let count_text = format!("{}", stack.count);
            let text_pos = rect.max - egui::vec2(4.0, 3.0);
            ui.painter().text(
                text_pos + egui::vec2(1.0, 1.0),
                egui::Align2::RIGHT_BOTTOM,
                &count_text,
                egui::FontId::proportional(12.0),
                egui::Color32::BLACK,
            );
            ui.painter().text(
                text_pos,
                egui::Align2::RIGHT_BOTTOM,
                &count_text,
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        }
    }
}

impl EngineApp {
    fn new(mode: Box<dyn GameMode>) -> Self {
        let settings = Settings::load();
        audio::set_volume(audio::SoundCategory::Master, settings.master_volume);
        audio::set_volume(audio::SoundCategory::Sfx, settings.sfx_volume);
        audio::set_volume(audio::SoundCategory::Ambient, settings.ambient_volume);
        audio::set_volume(audio::SoundCategory::Ui, settings.ui_volume);

        let mut registry = BlockRegistry::new();
        mode.register_blocks(&mut registry);
        let registry = Arc::new(registry);

        let mut materials = MaterialRegistry::with_builtins();
        mode.register_materials(&mut materials);
        let materials = Arc::new(materials);
        let generator = mode.create_generator(&registry);
        let environment = mode.create_environment();
        let avatar = mode.create_avatar();
        let spawn = mode.spawn_position();

        let mut cached_world_list = WorldManager::list_worlds();
        let server_profiles = WorldManager::list_server_profiles();
        let (host_profile_name_input, active_host_profile) = if let Some((name, prof)) = server_profiles.first() {
            (name.clone(), prof.clone())
        } else {
            ("Default Co-op".to_string(), ServerHostProfile::default())
        };
        if cached_world_list.is_empty() {
            let gen = LithosphereWorldGen::new(42, &registry);
            let (sy, _) = gen.surface_height(0, 0);
            let spawn_arr = [0.5, (sy + 2).max(10) as f32, 0.5];
            if let Ok(meta) = WorldManager::create_world("Мой мир", 42, "Survival", "Lithosphere", spawn_arr) {
                cached_world_list.push(meta);
            }
        }
        let selected_world_id = cached_world_list.first().map(|w| w.id.clone());
        let current_world = cached_world_list.first().cloned();

        let mut world = World::new();
        if let Some(meta) = &current_world {
            let edits_path = meta.edits_file_path();
            if edits_path.exists() {
                let _ = world.load_edits(&edits_path, &registry);
            }
        } else {
            let world_save_path = std::path::Path::new("world.dat");
            if let Err(err) = world.load_edits(world_save_path, &registry) {
                eprintln!("failed to load world.dat: {err}");
            }
        }

        let initial_spawn = current_world
            .as_ref()
            .map(|w| Vec3::from_array(w.player_pos))
            .unwrap_or(spawn);

        let (spawn_chunk, _) = world_to_chunk(initial_spawn.floor().as_ivec3());
        world.ensure_chunks_around(
            IVec3::new(spawn_chunk.x, 0, spawn_chunk.z),
            2.min(settings.render_distance),
            generator.as_ref(),
        );

        let streamer = WorldStreamer::new(Arc::clone(&generator), Arc::clone(&registry));

        let mut camera = Camera::new(initial_spawn + Vec3::Y * PLAYER_EYE_HEIGHT);
        camera.fov_radians = settings.fov_degrees.to_radians();

        let egui_ctx = egui::Context::default();
        setup_cyrillic_font(&egui_ctx);

        let inventory = Inventory::new();

        let mut commands = CommandRegistry::new();
        register_built_in_commands(&mut commands);
        let commands = Arc::new(commands);

        let now = Instant::now();
        let controller = CharacterController::new(initial_spawn, MovementConfig::default());
        let mut app = Self {
            mode,
            registry,
            materials,
            commands,
            generator,
            world,
            streamer,
            target_yaw: camera.yaw,
            target_pitch: camera.pitch,
            camera,
            velocity: Vec3::ZERO,
            on_ground: false,
            controller,
            jump_just_pressed: false,
            hand_sway: Vec3::ZERO,
            player_mode: PlayerMode::Survival,
            fly_mode: false,
            allow_fly: false,
            last_space_press: None,
            inventory,
            break_target: None,
            break_progress: 0.0,
            break_time: 1.0,
            inventory_open: false,
            held_stack: None,
            connect_ip_input: settings.last_connected_ip.clone(),
            server_port_input: "25565".to_string(),
            player_name_input: settings.player_name.clone(),
            settings,
            screen: MenuScreen::Title,
            current_world,
            cached_world_list,
            selected_world_id,
            create_world_name: "Новый мир".to_string(),
            create_world_seed: "42".to_string(),
            create_world_mode: "Survival".to_string(),
            create_world_gen: "Lithosphere".to_string(),
            chat_open: false,
            chat_input: String::new(),
            chat_focus_requested: false,
            chat_history: Vec::new(),
            rebinding: None,
            input: InputState::new(),
            window: None,
            renderer: None,
            egui_ctx,
            egui_state: None,
            last_frame: now,
            fps_timer: now,
            frame_counter: 0,
            displayed_fps: 0,
            time_of_day: 10.0,
            time_speed: 0.05,
            environment,
            avatar,
            perspective: CameraPerspective::FirstPerson,
            walk_phase: 0.0,
            arm_swing_progress: 0.0,
            arm_swing_active: false,
            lmb_held: false,
            server_host: None,
            game_client: None,
            host_remote_players: HashMap::new(),
            discovery_beacon: None,
            discovery_scanner: DiscoveryScanner::new(),
            net_status: None,
            last_net_sync: now,
            panorama_angle: 0.0,
            player_pos: camera.position,
            player_yaw: camera.yaw,
            player_pitch: camera.pitch,
            return_screen: MenuScreen::Title,
            health: 100.0,
            last_damage_time: None,
            last_attack_time: None,
            creative_search: String::new(),
            last_attacker_name: None,
            last_damage_info: None,
            last_hitmarker: None,
            camera_flinch: 0.0,
            death_fall_progress: 0.0,
            last_regen_sync: now,
            // ===== [TEMPORARY PHYSICS TEST: START] =====
            test_physics_blocks: Vec::new(),
            // ===== [TEMPORARY PHYSICS TEST: END] =====
            session_type: SessionType::Solo,
            server_profiles,
            selected_server_profile_idx: 0,
            active_host_profile,
            host_profile_name_input,
            host_active_tab: HostConfigTab::Network,
            host_status_msg: None,
            join_tab: JoinTab::Lan,
            join_password_input: String::new(),
            dropped_items: Vec::new(),
            next_dropped_item_id: 1,
        };

        app.push_chat(
            "Добро пожаловать в Voxel Engine! Нажмите [T] для чата или [/] для команд.",
            egui::Color32::from_rgb(120, 220, 120),
        );
        app.push_chat(
            "Подсказка: [F5] переключает вид от 3-го лица (персонаж в стиле Unturned).",
            egui::Color32::from_rgb(255, 215, 120),
        );
        app.push_chat(
            "Подсказка: /gamemode creative переключает в креатив (двойной пробел — полёт).",
            egui::Color32::from_rgb(180, 210, 255),
        );
        // ===== [TEMPORARY PHYSICS TEST: START] =====
        app.push_chat(
            "Подсказка: [G] бросает физический блок перед вами (тест физики и коллизий).",
            egui::Color32::from_rgb(100, 220, 255),
        );
        // ===== [TEMPORARY PHYSICS TEST: END] =====

        app
    }

    fn push_chat(&mut self, text: impl Into<String>, color: egui::Color32) {
        let text = text.into();
        for line in text.lines() {
            self.chat_history.push(ChatEntry {
                text: line.to_string(),
                color,
                timestamp: Instant::now(),
            });
        }
        if self.chat_history.len() > 80 {
            self.chat_history.drain(0..20);
        }
    }

    fn selected_block(&self) -> BlockId {
        self.inventory.selected_block()
    }

    fn find_pvp_target(&self, max_reach: f32) -> Option<(u32, String, f32)> {
        let eye = self.camera.position;
        let fwd = self.camera.forward().normalize();

        let remote_players: Vec<(u32, String, Vec3)> = if self.server_host.is_some() {
            self.host_remote_players
                .values()
                .map(|p| (p.id, p.name.clone(), p.current_pos))
                .collect()
        } else if let Some(client) = &self.game_client {
            client
                .remote_players
                .values()
                .map(|p| (p.id, p.name.clone(), p.current_pos))
                .collect()
        } else {
            Vec::new()
        };

        let mut closest = None;
        let mut min_dist = max_reach;

        for (id, name, feet) in remote_players {
            let center = feet + Vec3::new(0.0, 0.9, 0.0);
            let to_target = center - eye;
            let dist = to_target.length();
            if dist > max_reach {
                continue;
            }
            let proj = to_target.dot(fwd);
            if proj <= 0.0 {
                continue;
            }
            let perp_dist = (to_target - fwd * proj).length();
            if perp_dist <= 0.85 && dist < min_dist {
                min_dist = dist;
                closest = Some((id, name, dist));
            }
        }


        closest
    }

    fn player_feet(&self) -> Vec3 {
        self.controller.position
    }

    fn set_player_feet(&mut self, feet: Vec3) {
        self.controller.position = feet;
        self.controller.velocity = Vec3::ZERO;
        self.camera.position = feet + Vec3::Y * (self.controller.config.standing_height * 0.90);
        self.player_pos = feet;
    }

    fn spawn_dropped_item(&mut self, block_id: BlockId, count: u32, pos: Vec3, vel: Vec3) {
        if block_id == BlockId::AIR || count == 0 {
            return;
        }
        let id = self.next_dropped_item_id;
        self.next_dropped_item_id = self.next_dropped_item_id.wrapping_add(1);
        self.dropped_items.push(DroppedItem {
            id,
            position: pos,
            velocity: vel,
            block_id,
            count,
            age: 0.0,
            pickup_delay: 0.5,
        });
    }

    fn set_cursor_lock(&mut self, lock: bool) {
        let Some(window) = &self.window else {
            return;
        };
        if lock {
            let locked = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            if locked {
                window.set_cursor_visible(false);
                self.input.cursor_locked = true;
                self.input.mouse_delta = (0.0, 0.0);
            }
        }
 else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.input.cursor_locked = false;
            self.input.mouse_delta = (0.0, 0.0);
        }
    }

    fn open_pause(&mut self) {
        self.close_chat();
        self.screen = MenuScreen::Pause;
        self.rebinding = None;
        self.input.keys.clear();
        self.set_cursor_lock(false);
    }

    fn resume_game(&mut self) {
        self.screen = MenuScreen::Playing;
        self.close_chat();
        self.rebinding = None;
        self.set_cursor_lock(true);
    }

    fn open_select_world(&mut self) {
        self.cached_world_list = WorldManager::list_worlds();
        if self.selected_world_id.is_none()
            || !self
                .cached_world_list
                .iter()
                .any(|w| Some(&w.id) == self.selected_world_id.as_ref())
        {
            self.selected_world_id = self.cached_world_list.first().map(|w| w.id.clone());
        }
        self.screen = MenuScreen::SelectWorld;
    }

    fn load_world(&mut self, mut meta: WorldMeta) {
        self.save_current_world();

        self.world.clear();
        if let Some(r) = &mut self.renderer {
            r.clear_chunk_meshes();
        }

        let mut survival = game_survival::SurvivalMode::default();
        survival.seed = meta.seed;
        survival.generator_type = meta.generator.clone();
        let generator = survival.create_generator(&self.registry);

        self.streamer = WorldStreamer::new(Arc::clone(&generator), Arc::clone(&self.registry));
        self.generator = Arc::clone(&generator);

        let edits_path = meta.edits_file_path();
        if edits_path.exists() {
            let _ = self.world.load_edits(&edits_path, &self.registry);
        }

        if meta.mode == "Creative" {
            self.player_mode = PlayerMode::Creative;
            self.allow_fly = true;
            self.fly_mode = true;
        } else {
            self.player_mode = PlayerMode::Survival;
            self.allow_fly = false;
            self.fly_mode = false;
        }

        let spawn = Vec3::from_array(meta.player_pos);
        self.player_pos = spawn;
        self.controller.position = spawn;
        self.controller.velocity = Vec3::ZERO;
        self.controller.on_ground = false;
        self.controller.smoothed_eye_y = spawn.y + self.controller.config.standing_height * 0.90;
        self.camera.position = spawn;
        self.camera.yaw = meta.player_yaw;
        self.camera.pitch = meta.player_pitch;
        self.target_yaw = meta.player_yaw;
        self.target_pitch = meta.player_pitch;
        self.velocity = Vec3::ZERO;
        self.health = meta.player_health;

        let (spawn_chunk, _) = world_to_chunk(spawn.floor().as_ivec3());
        self.world.ensure_chunks_around(
            IVec3::new(spawn_chunk.x, 0, spawn_chunk.z),
            2.min(self.settings.render_distance),
            generator.as_ref(),
        );

        meta.last_played = WorldMeta::current_timestamp_string();
        let _ = WorldManager::save_world(&mut meta, None);
        let world_name = meta.name.clone();
        self.current_world = Some(meta);
        self.mode = Box::new(survival);

        self.resume_game();
        self.push_chat(
            &format!("[Мир] Загружен мир: {world_name}"),
            egui::Color32::from_rgb(100, 220, 100),
        );
    }

    fn save_current_world(&mut self) {
        if let Some(meta) = &mut self.current_world {
            meta.player_pos = self.camera.position.to_array();
            meta.player_yaw = self.camera.yaw;
            meta.player_pitch = self.camera.pitch;
            meta.player_health = self.health;
            meta.last_played = WorldMeta::current_timestamp_string();
            let _ = self.world.save_edits(&meta.edits_file_path(), &self.registry);
            let _ = WorldManager::save_world(meta, None);
        } else {
            let _ = self.world.save_edits(std::path::Path::new("world.dat"), &self.registry);
        }
    }

    fn open_chat(&mut self, prefill: &str) {
        if self.screen != MenuScreen::Playing {
            return;
        }
        self.chat_open = true;
        self.chat_input = prefill.to_string();
        self.chat_focus_requested = true;
        self.input.keys.clear();
        self.set_cursor_lock(false);
    }

    fn close_chat(&mut self) {
        self.chat_open = false;
        self.chat_input.clear();
        self.chat_focus_requested = false;
        if self.screen == MenuScreen::Playing {
            self.set_cursor_lock(true);
        }
    }

    fn apply_and_broadcast_block(&mut self, pos: IVec3, block_id: BlockId) {
        self.world.set_block(pos, block_id);

        let (chunk_pos, [lx, ly, lz]) = world_to_chunk(pos);
        let mut affected = vec![chunk_pos];
        if lx == 0 { affected.push(chunk_pos + IVec3::new(-1, 0, 0)); }
        if lx == 15 { affected.push(chunk_pos + IVec3::new(1, 0, 0)); }
        if ly == 0 { affected.push(chunk_pos + IVec3::new(0, -1, 0)); }
        if ly == 15 { affected.push(chunk_pos + IVec3::new(0, 1, 0)); }
        if lz == 0 { affected.push(chunk_pos + IVec3::new(0, 0, -1)); }
        if lz == 15 { affected.push(chunk_pos + IVec3::new(0, 0, 1)); }

        let immediate = self.world.rebuild_immediate_meshes(&affected, &self.registry);
        if let Some(renderer) = &mut self.renderer {
            if !immediate.is_empty() {
                renderer.upload_chunk_meshes(immediate);
            }
        }

        let arr = [pos.x, pos.y, pos.z];
        let bid = block_id.0;
        if let Some(host) = &mut self.server_host {
            host.broadcast(&Packet::BlockChange {
                pos: arr,
                block_id: bid,
            });
            host.edits.retain(|(p, _)| *p != arr);
            host.edits.push((arr, bid));
        }
        if let Some(client) = &mut self.game_client {
            client.send_block_change(arr, bid);
        }
    }

    fn submit_chat(&mut self) {
        let trimmed = self.chat_input.trim().to_string();
        self.chat_input.clear();
        if trimmed.is_empty() {
            self.close_chat();
            return;
        }

        if trimmed.starts_with('/') {
            self.push_chat(&trimmed, egui::Color32::from_rgb(255, 230, 140));
            self.execute_command(&trimmed);
        } else {
            let my_name = if self.server_host.is_some() {
                format!("{}(Хост)", self.settings.player_name)
            } else {
                self.settings.player_name.clone()
            };
            self.push_chat(format!("<{my_name}> {trimmed}"), egui::Color32::WHITE);

            if let Some(host) = &mut self.server_host {
                host.broadcast(&Packet::ChatMessage {
                    sender_id: 0,
                    sender_name: my_name,
                    text: trimmed,
                });
            } else if let Some(client) = &mut self.game_client {
                client.send_chat(&trimmed);
            }
        }

        self.close_chat();
    }

    fn execute_command(&mut self, line: &str) {
        struct HostAdapter<'a> {
            app: &'a mut EngineApp,
        }

        impl<'a> CommandHost for HostAdapter<'a> {
            fn player_position(&self) -> Vec3 {
                self.app.camera.position
            }

            fn teleport_player(&mut self, pos: Vec3) {
                self.app.camera.position = pos;
                self.app.velocity = Vec3::ZERO;
            }

            fn player_mode(&self) -> PlayerMode {
                self.app.player_mode
            }

            fn set_player_mode(&mut self, mode: PlayerMode) {
                self.app.player_mode = mode;
                if mode == PlayerMode::Creative {
                    self.app.allow_fly = true;
                } else {
                    self.app.allow_fly = false;
                    self.app.fly_mode = false;
                }
            }

            fn is_flying(&self) -> bool {
                self.app.fly_mode
            }

            fn set_flying(&mut self, flying: bool) {
                self.app.fly_mode = flying;
                self.app.velocity = Vec3::ZERO;
            }

            fn can_fly(&self) -> bool {
                self.app.allow_fly || self.app.player_mode == PlayerMode::Creative
            }

            fn set_can_fly(&mut self, allowed: bool) {
                self.app.allow_fly = allowed;
                if !allowed && self.app.player_mode != PlayerMode::Creative {
                    self.app.fly_mode = false;
                }
            }

            fn block_registry(&self) -> &BlockRegistry {
                &self.app.registry
            }

            fn set_selected_block(&mut self, id: BlockId) {
                if let Some(pos) = (0..9).find(|&i| self.app.inventory.slots[i].map(|s| s.block_id) == Some(id)) {
                    self.app.inventory.selected_hotbar = pos;
                } else {
                    self.app.inventory.slots[self.app.inventory.selected_hotbar] = Some(ItemStack::new(id, 64));
                }
            }

            fn clear_chat(&mut self) {
                self.app.chat_history.clear();
            }

            fn save_world(&mut self) -> Result<usize, String> {
                let path = if let Some(meta) = &self.app.current_world {
                    meta.edits_file_path()
                } else {
                    std::path::PathBuf::from("world.dat")
                };
                self.app
                    .world
                    .save_edits(&path, &self.app.registry)
                    .map_err(|e| e.to_string())
            }

            fn time_of_day(&self) -> f32 {
                self.app.time_of_day
            }

            fn set_time_of_day(&mut self, time: f32) {
                self.app.time_of_day = time.rem_euclid(24.0);
            }

            fn material_registry(&self) -> &MaterialRegistry {
                &self.app.materials
            }

            fn heal_player(&mut self, amount: f32) {
                self.app.health = (self.app.health + amount).min(100.0);
                audio::play_sound(audio::SoundEffect::Heal);
                let my_id = if self.app.server_host.is_some() {
                    0
                } else if let Some(client) = &self.app.game_client {
                    client.my_id
                } else {
                    0
                };
                let hp_pkt = Packet::PlayerHealth {
                    player_id: my_id,
                    health: self.app.health,
                };
                if let Some(host) = &mut self.app.server_host {
                    host.broadcast(&hp_pkt);
                } else if let Some(client) = &mut self.app.game_client {
                    let _ = client.send_packet(&hp_pkt);
                }
            }

            fn find_player(&self, query: &str) -> Option<(String, Vec3)> {
                let q = query.trim().to_lowercase();
                if self.app.server_host.is_some() {
                    for p in self.app.host_remote_players.values() {
                        if p.name.to_lowercase().contains(&q) {
                            return Some((p.name.clone(), p.current_pos));
                        }
                    }
                } else if let Some(client) = &self.app.game_client {
                    for p in client.remote_players.values() {
                        if p.name.to_lowercase().contains(&q) {
                            return Some((p.name.clone(), p.current_pos));
                        }
                    }
                }
                None
            }
        }


        let commands = Arc::clone(&self.commands);
        let mut adapter = HostAdapter { app: self };
        let result = commands.execute(line, &mut adapter);
        match result {
            Ok(msg) => {
                if !msg.is_empty() {
                    adapter.app.push_chat(msg, egui::Color32::from_rgb(130, 230, 130));
                }
            }
            Err(err) => {
                adapter.app.push_chat(err, egui::Color32::from_rgb(255, 120, 120));
            }
        }

    }

    fn apply_settings(&mut self) {
        self.camera.fov_radians = self.settings.fov_degrees.to_radians();
        if let Some(renderer) = &mut self.renderer {
            renderer.set_vsync(self.settings.vsync);
        }
        self.settings.save();
    }

    fn toggle_flight_mode(&mut self) {
        let allowed = self.player_mode == PlayerMode::Creative || self.allow_fly;
        if allowed {
            self.fly_mode = !self.fly_mode;
            self.velocity = Vec3::ZERO;
            let status = if self.fly_mode { "включён" } else { "выключен" };
            self.push_chat(
                format!("Режим полёта {status}"),
                egui::Color32::from_rgb(200, 200, 255),
            );
        } else {
            self.push_chat(
                "Полёт недоступен в выживании. Переключите режим: /gamemode creative",
                egui::Color32::from_rgb(255, 190, 120),
            );
        }
    }

    fn update(&mut self, dt: f32) {
        let dt = dt.min(0.05);

        // In offline Solo mode, Pause (ESC) genuinely freezes the entire simulation!
        let is_solo_paused = self.screen == MenuScreen::Pause && self.session_type == SessionType::Solo;
        if is_solo_paused {
            return;
        }

        self.time_of_day = (self.time_of_day + self.time_speed * dt).rem_euclid(24.0);

        if self.screen == MenuScreen::Title
            || self.screen == MenuScreen::SelectWorld
            || self.screen == MenuScreen::CreateWorld
            || self.screen == MenuScreen::HostServer
            || self.screen == MenuScreen::JoinServer
        {
            self.panorama_angle += dt * 0.08;
            let radius = 18.0;
            self.camera.position = self.player_pos + Vec3::new(
                self.panorama_angle.cos() * radius,
                16.0,
                self.panorama_angle.sin() * radius,
            );
            self.camera.yaw = self.panorama_angle + std::f32::consts::FRAC_PI_2 + 0.25;
            self.camera.pitch = -0.12 + (self.panorama_angle * 0.5).sin() * 0.03;
            self.target_yaw = self.camera.yaw;
            self.target_pitch = self.camera.pitch;
        } else if self.screen == MenuScreen::Playing && !self.chat_open && self.input.cursor_locked && self.health > 0.0 {
            let (dx, dy) = self.input.mouse_delta;
            let sens = 0.0020 * self.settings.mouse_sensitivity;
            let y_sign = if self.settings.invert_y { -1.0 } else { 1.0 };
            self.target_yaw += dx * sens;
            self.target_pitch = (self.target_pitch - dy * sens * y_sign).clamp(-1.54, 1.54);

            let target_sway_x = (-dx * 0.00035).clamp(-0.06, 0.06);
            let target_sway_y = (dy * 0.00035).clamp(-0.06, 0.06);
            let sway_rate = (15.0 * dt).min(1.0);
            self.hand_sway.x += (target_sway_x - self.hand_sway.x) * sway_rate;
            self.hand_sway.y += (target_sway_y - self.hand_sway.y) * sway_rate;
            self.hand_sway *= (1.0 - 5.0 * dt).max(0.0);

            if self.settings.mouse_smoothing <= 0.001 {
                self.camera.yaw = self.target_yaw;
                self.camera.pitch = self.target_pitch;
            } else {
                let smoothness = self.settings.mouse_smoothing.clamp(0.0, 0.95);
                let rate = 8.0 + (1.0 - smoothness) * 55.0;
                let alpha = 1.0 - (-rate * dt).exp();
                self.camera.yaw += (self.target_yaw - self.camera.yaw) * alpha;
                self.camera.pitch += (self.target_pitch - self.camera.pitch) * alpha;
            }
        }

        self.input.mouse_delta = (0.0, 0.0);

        if self.health <= 0.0 {
            if self.input.cursor_locked {
                self.set_cursor_lock(false);
            }
            self.death_fall_progress = (self.death_fall_progress + dt * 3.0).min(1.0);
            self.break_target = None;
            self.break_progress = 0.0;
            self.arm_swing_active = false;
        } else {
            self.death_fall_progress = 0.0;
        }

        self.camera_flinch = (self.camera_flinch - dt * 2.0).max(0.0);

        // Passive gradual health regeneration (out of combat for 4s)
        if self.health > 0.0 && self.health < 100.0 {
            let out_of_combat = self.last_damage_time.map(|t| t.elapsed().as_secs_f32() > 4.0).unwrap_or(true);
            if out_of_combat {
                let prev_hp = self.health;
                self.health = (self.health + 3.0 * dt).min(100.0);
                if self.last_regen_sync.elapsed().as_millis() >= 1000 || (prev_hp < 100.0 && self.health >= 100.0) {
                    self.last_regen_sync = Instant::now();
                    let my_id = if self.server_host.is_some() { 0 } else if let Some(client) = &self.game_client { client.my_id } else { 0 };
                    let hp_pkt = Packet::PlayerHealth { player_id: my_id, health: self.health };
                    if let Some(host) = &mut self.server_host {
                        host.broadcast(&hp_pkt);
                    } else if let Some(client) = &mut self.game_client {
                        let _ = client.send_packet(&hp_pkt);
                    }
                }
            }
        }

        if self.screen == MenuScreen::Playing && !self.chat_open && !self.inventory_open && self.health > 0.0 {
            let mut wish_xz = Vec3::ZERO;
            if self.input.is_bound_down(&self.settings.keybinds.forward) {
                wish_xz += self.camera.forward_xz();
            }
            if self.input.is_bound_down(&self.settings.keybinds.backward) {
                wish_xz -= self.camera.forward_xz();
            }
            if self.input.is_bound_down(&self.settings.keybinds.right) {
                wish_xz += self.camera.right_xz();
            }
            if self.input.is_bound_down(&self.settings.keybinds.left) {
                wish_xz -= self.camera.right_xz();
            }
            wish_xz = wish_xz.normalize_or_zero();

            let sprinting = self.input.is_bound_down(&self.settings.keybinds.sprint);
            let crouching = self.input.is_bound_down(&self.settings.keybinds.down);
            let jump_held = self.input.is_bound_down(&self.settings.keybinds.jump_up);
            let jump_pressed = self.jump_just_pressed;
            self.jump_just_pressed = false;

            let ctrl_input = ControllerInput {
                wish_dir: wish_xz,
                jump: jump_held,
                jump_pressed,
                sprint: sprinting,
                crouch: crouching,
                fly: self.fly_mode,
                fly_up: jump_held,
                fly_down: crouching,
                fly_speed_multiplier: if sprinting { 2.2 } else { 1.0 },
                look_yaw: self.camera.yaw,
            };

            let out = self.controller.step(
                &ctrl_input,
                &self.world,
                &self.registry,
                &self.materials,
                dt,
                1.0,
            );

            self.player_pos = out.position;
            self.velocity = out.velocity;
            self.on_ground = out.on_ground;
            self.camera.position = out.eye_position;
            self.camera.roll = out.strafe_roll;

            // Handle step sounds
            if let Some(step) = out.step_event {
                let effect = match step {
                    StepType::WaterSwim | StepType::WaterSplash => audio::SoundEffect::Footstep,
                    StepType::Footstep(block_id) => {
                        if let Some(def) = self.registry.get(block_id) {
                            let name = def.name.to_lowercase();
                            if name.contains("grass") {
                                audio::SoundEffect::FootstepGrass
                            } else if name.contains("stone")
                                || name.contains("cobble")
                                || name.contains("brick")
                                || name.contains("andesite")
                                || name.contains("furnace")
                            {
                                audio::SoundEffect::FootstepStone
                            } else if name.contains("wood")
                                || name.contains("planks")
                                || name.contains("bookshelf")
                                || name.contains("crafting")
                            {
                                audio::SoundEffect::FootstepWood
                            } else if name.contains("sand") {
                                audio::SoundEffect::FootstepSand
                            } else if name.contains("gravel") {
                                audio::SoundEffect::FootstepGravel
                            } else if name.contains("dirt") {
                                audio::SoundEffect::FootstepDirt
                            } else if name.contains("leaves") {
                                audio::SoundEffect::FootstepLeaves
                            } else {
                                audio::SoundEffect::Footstep
                            }
                        } else {
                            audio::SoundEffect::FootstepGrass
                        }
                    }
                };
                audio::play_sound(effect);
            }
        } else if self.health <= 0.0 && self.screen == MenuScreen::Playing && !self.fly_mode {
            let ctrl_input = ControllerInput {
                wish_dir: Vec3::ZERO,
                jump: false,
                jump_pressed: false,
                sprint: false,
                crouch: false,
                fly: false,
                fly_up: false,
                fly_down: false,
                fly_speed_multiplier: 1.0,
                look_yaw: self.camera.yaw,
            };
            let out = self.controller.step(
                &ctrl_input,
                &self.world,
                &self.registry,
                &self.materials,
                dt,
                1.0,
            );
            self.player_pos = out.position;
            self.velocity = out.velocity;
            self.on_ground = out.on_ground;
        }

        // ===== [TEMPORARY PHYSICS TEST: START] =====
        let grav = Vec3::new(0.0, -20.0, 0.0);
        for block in &mut self.test_physics_blocks {
            if block.transform.local_velocity.length_squared() > 0.001 {
                block.transform.local_velocity += grav * dt;
                block.transform.local_velocity.x *= (1.0 - 0.8 * dt).max(0.0);
                block.transform.local_velocity.z *= (1.0 - 0.8 * dt).max(0.0);

                let old_pos = block.transform.world_position();
                let mut new_pos = old_pos + block.transform.local_velocity * dt;

                let bottom_center = new_pos + Vec3::new(0.5, 0.05, 0.5);
                let check_voxel = bottom_center.floor().as_ivec3();
                if self.registry.is_solid(self.world.get_block(check_voxel)) {
                    new_pos.y = check_voxel.y as f32 + 1.0;
                    block.transform.local_velocity = Vec3::ZERO;
                    audio::play_sound(audio::SoundEffect::FootstepStone);
                }
                block.transform.set_world_position(new_pos);
            }
        }

        // Player collision: allow player to stand on and collide with the physical block
        let feet = self.player_feet();
        for block in &self.test_physics_blocks {
            let b_min = block.transform.world_position();
            let b_max = b_min + Vec3::splat(1.0);
            if feet.x >= b_min.x - 0.28
                && feet.x <= b_max.x + 0.28
                && feet.z >= b_min.z - 0.28
                && feet.z <= b_max.z + 0.28
                && feet.y >= b_max.y - 0.35
                && feet.y <= b_max.y + 0.30
                && self.velocity.y <= 0.0
            {
                self.set_player_feet(Vec3::new(feet.x, b_max.y, feet.z));
                self.velocity.y = 0.0;
                self.on_ground = true;
                break;
            }
        }
        // ===== [TEMPORARY PHYSICS TEST: END] =====


        let horiz_speed = Vec3::new(self.velocity.x, 0.0, self.velocity.z).length();
        if horiz_speed > 0.15 && (self.on_ground || self.fly_mode || self.controller.in_water) {
            self.walk_phase += horiz_speed * dt * 5.2;
        } else {
            self.walk_phase *= (1.0 - 10.0 * dt).max(0.0);
        }

        if self.arm_swing_active {
            self.arm_swing_progress += dt * 4.5;
            if self.arm_swing_progress >= 1.0 {
                if self.lmb_held {
                    self.arm_swing_progress -= 1.0;
                } else {
                    self.arm_swing_progress = 0.0;
                    self.arm_swing_active = false;
                }
            }
        }


        // Gradual block mining logic
        if self.screen == MenuScreen::Playing && !self.chat_open && !self.inventory_open && self.lmb_held && self.health > 0.0 {
            let hit = self.world.raycast(self.camera.position, self.camera.forward(), 6.5, &self.registry);
            if let Some(hit) = hit {
                let block_name = self.registry.get(hit.block).map(|d| d.name.clone()).unwrap_or_default();
                if self.player_mode == PlayerMode::Creative {
                    self.apply_and_broadcast_block(hit.hit_pos, BlockId::AIR);
                    audio::play_block_break(&block_name, Some(hit.hit_pos.as_vec3()));
                    let local_meshes = self.world.rebuild_dirty_meshes(&self.registry, 8);
                    if !local_meshes.is_empty() {
                        if let Some(renderer) = &mut self.renderer {
                            renderer.upload_chunk_meshes(local_meshes);
                        }
                    }
                    self.break_target = None;
                    self.break_progress = 0.0;
                } else if Some(hit.hit_pos) == self.break_target {
                    let prev_p = self.break_progress;
                    self.break_progress += dt / self.break_time.max(0.05);
                    if (prev_p * self.break_time / 0.25).floor() < (self.break_progress * self.break_time / 0.25).floor() {
                        audio::play_block_hit(&block_name, Some(hit.hit_pos.as_vec3()));
                    }
                    if self.break_progress >= 1.0 {
                        self.apply_and_broadcast_block(hit.hit_pos, BlockId::AIR);
                        audio::play_block_break(&block_name, Some(hit.hit_pos.as_vec3()));
                        let drop_pos = hit.hit_pos.as_vec3() + Vec3::new(0.5, 0.4, 0.5);
                        let rnd_x = ((self.time_of_day * 133.0).sin()) * 1.2;
                        let rnd_z = ((self.time_of_day * 277.0).cos()) * 1.2;
                        self.spawn_dropped_item(hit.block, 1, drop_pos, Vec3::new(rnd_x, 2.8, rnd_z));
                        self.break_target = None;
                        self.break_progress = 0.0;
                        let local_meshes = self.world.rebuild_dirty_meshes(&self.registry, 8);
                        if !local_meshes.is_empty() {
                            if let Some(renderer) = &mut self.renderer {
                                renderer.upload_chunk_meshes(local_meshes);
                            }
                        }
                    }
                } else {
                    self.break_target = Some(hit.hit_pos);
                    self.break_progress = 0.0;
                    let hardness = self.registry.get(hit.block).map(|d| d.hardness).unwrap_or(1.0);
                    self.break_time = (hardness * 0.8).max(0.1);
                    audio::play_block_hit(&block_name, Some(hit.hit_pos.as_vec3()));
                }
            } else {
                self.break_target = None;
                self.break_progress = 0.0;
            }
        } else {
            self.break_target = None;
            self.break_progress = 0.0;
        }

        // Update dropped items in world (physics, magnetic pull, pickup)
        if self.screen == MenuScreen::Playing {
            let player_feet = self.player_feet();
            let player_center = player_feet + Vec3::new(0.0, 0.9, 0.0);
            let mut picked_up_any = false;

            self.dropped_items.retain_mut(|item| {
                item.age += dt;
                if item.pickup_delay > 0.0 {
                    item.pickup_delay -= dt;
                }

                // Gravity & simple voxel collision
                item.velocity.y -= 14.0 * dt;
                let next_pos = item.position + item.velocity * dt;
                let check_block = next_pos.floor().as_ivec3();
                let block = self.world.get_block(check_block);
                let is_solid = self.registry.is_solid(block);

                if is_solid {
                    item.velocity.y = 0.0;
                    item.velocity.x *= 0.60;
                    item.velocity.z *= 0.60;
                    let ground_y = check_block.y as f32 + 1.05;
                    if item.position.y < ground_y {
                        item.position.y = ground_y;
                    }
                } else {
                    item.position = next_pos;
                }

                // Magnetic attraction and pickup
                let dist = item.position.distance(player_center);
                if item.pickup_delay <= 0.0 && dist < 2.0 {
                    let dir = (player_center - item.position).normalize();
                    item.position += dir * (7.5 * dt);

                    if dist < 0.65 {
                        let excess = self.inventory.add_item(item.block_id, item.count);
                        if excess == 0 {
                            picked_up_any = true;
                            return false; // Despawn item
                        } else {
                            item.count = excess;
                        }
                    }
                }

                item.age < 300.0 // 5 minute despawn
            });

            if picked_up_any {
                audio::play_event_at("item.pickup", player_center);
            }
        }

        audio::set_listener(self.camera.position, self.camera.forward(), self.camera.right_xz());
        if self.screen == MenuScreen::Playing {
            audio::update_ambient(self.time_of_day, self.player_feet().y);
        } else {
            audio::stop_ambient();
        }

        let (player_chunk, _) = world_to_chunk(self.camera.position.floor().as_ivec3());
        let center = IVec3::new(player_chunk.x, 0, player_chunk.z);

        self.world.request_chunks_around(
            center,
            self.settings.render_distance,
            self.generator.as_ref(),
            &self.streamer,
            16,
        );
        self.world.dispatch_dirty_meshes(center, &self.streamer, 12);

        let ready_meshes = self.world.poll_streamer(&self.streamer, center, 4);
        let unloaded = self.world.unload_outside(center, self.settings.render_distance);

        if let Some(renderer) = &mut self.renderer {
            if !unloaded.is_empty() {
                renderer.unload_chunks(&unloaded);
            }
            if !ready_meshes.is_empty() {
                renderer.upload_chunk_meshes(ready_meshes);
            }
        }


        // Network tick
        self.discovery_scanner.poll();

        if self.server_host.is_some() {
            let client_count = self.server_host.as_ref().unwrap().client_count() as u8;
            if let Some(beacon) = &mut self.discovery_beacon {
                beacon.tick(client_count + 1);
            }

            let events = self.server_host.as_mut().unwrap().poll_events();
            let mut host_block_changed = false;
            let mut to_broadcast = Vec::new();
            let mut to_broadcast_except = Vec::new();

            for event in events {
                match event {
                    HostEvent::ClientJoined { id, name, .. } => {
                        self.push_chat(
                            format!("+ {} вошёл в игру", name),
                            egui::Color32::from_rgb(120, 220, 120),
                        );
                        self.host_remote_players
                            .insert(id, RemotePlayer::new(id, name, Vec3::ZERO));
                    }
                    HostEvent::ClientLeft { id, name } => {
                        self.push_chat(format!("- {} вышел из игры", name), egui::Color32::GRAY);
                        self.host_remote_players.remove(&id);
                    }
                    HostEvent::PlayerState {
                        id,
                        pos,
                        yaw,
                        pitch,
                        walk_phase,
                        is_moving,
                        arm_swing,
                        held_block,
                    } => {
                        let target_pos = Vec3::from_array(pos);
                        let player = self
                            .host_remote_players
                            .entry(id)
                            .or_insert_with(|| RemotePlayer::new(id, format!("Player_{id}"), target_pos));
                        player.target_pos = target_pos;
                        player.target_yaw = yaw;
                        player.target_pitch = pitch;
                        player.walk_phase = walk_phase;
                        player.is_moving = is_moving;
                        player.arm_swing = arm_swing;
                        player.held_block = held_block;

                        to_broadcast_except.push((
                            id,
                            Packet::PlayerState {
                                id,
                                pos,
                                yaw,
                                pitch,
                                walk_phase,
                                is_moving,
                                arm_swing,
                                held_block,
                            },
                        ));
                    }
                    HostEvent::BlockChange { pos, block_id } => {
                        self.world.set_block(IVec3::from_array(pos), BlockId(block_id));
                        host_block_changed = true;
                        to_broadcast.push(Packet::BlockChange { pos, block_id });
                    }
                    HostEvent::ChatMessage {
                        sender_id,
                        sender_name,
                        text,
                    } => {
                        self.push_chat(format!("<{sender_name}> {text}"), egui::Color32::WHITE);
                        to_broadcast_except.push((
                            sender_id,
                            Packet::ChatMessage {
                                sender_id,
                                sender_name,
                                text,
                            },
                        ));
                    }
                    HostEvent::HitPlayer {
                        attacker_id,
                        target_id,
                        damage,
                        knockback,
                    } => {
                        if target_id == 0 {
                            self.health = (self.health - damage).max(0.0);
                            self.velocity = Vec3::new(self.velocity.x * 0.25, 0.0, self.velocity.z * 0.25) + Vec3::from_array(knockback);
                            self.last_damage_time = Some(Instant::now());
                            self.camera_flinch = 0.08;
                            let attacker_name = self.host_remote_players.get(&attacker_id).map(|p| p.name.clone()).unwrap_or_else(|| format!("Игрок {attacker_id}"));
                            self.last_attacker_name = Some(attacker_name.clone());
                            self.last_damage_info = Some((attacker_name.clone(), damage, Instant::now()));
                            audio::play_sound(audio::SoundEffect::Hurt);
                            to_broadcast.push(Packet::PlayerHealth { player_id: 0, health: self.health });
                            if self.health <= 0.0 {
                                audio::play_sound(audio::SoundEffect::Death);
                                self.set_cursor_lock(false);
                                self.push_chat(format!("[PvP] {attacker_name} победил Вас!"), egui::Color32::from_rgb(255, 100, 100));
                                to_broadcast.push(Packet::ChatMessage {
                                    sender_id: 0,
                                    sender_name: "PvP".to_string(),
                                    text: format!("{attacker_name} победил {}!", self.settings.player_name),
                                });
                            }
                        } else {
                            if let Some(victim) = self.host_remote_players.get_mut(&target_id) {
                                victim.health = (victim.health - damage).max(0.0);
                                victim.hurt_timer = 0.35;
                            }
                            to_broadcast_except.push((
                                attacker_id,
                                Packet::HitPlayer {
                                    attacker_id,
                                    target_id,
                                    damage,
                                    knockback,
                                },
                            ));
                        }
                    }
                    HostEvent::PlayerHealth { player_id, health } => {
                        if let Some(player) = self.host_remote_players.get_mut(&player_id) {
                            player.health = health;
                        }
                        to_broadcast_except.push((player_id, Packet::PlayerHealth { player_id, health }));
                    }
                }
            }

            if let Some(host) = &mut self.server_host {
                for p in &to_broadcast {
                    host.broadcast(p);
                }
                for (except_id, p) in &to_broadcast_except {
                    host.broadcast_except(*except_id, p);
                }
            }

            if host_block_changed {
                let local_meshes = self.world.rebuild_dirty_meshes(&self.registry, 8);
                if !local_meshes.is_empty() {
                    if let Some(renderer) = &mut self.renderer {
                        renderer.upload_chunk_meshes(local_meshes);
                    }
                }
            }

            for player in self.host_remote_players.values_mut() {
                player.interpolate(dt);
            }

            // Sync host player state to clients at 25 Hz
            if self.last_net_sync.elapsed().as_millis() >= 40 {
                self.last_net_sync = Instant::now();
                let feet = self.player_feet();
                let horiz_speed = Vec3::new(self.velocity.x, 0.0, self.velocity.z).length();
                let held_block = self.inventory.selected_item().filter(|s| s.block_id != BlockId::AIR).map(|s| s.block_id.0).unwrap_or(0);
                if let Some(host) = &mut self.server_host {
                    host.broadcast(&Packet::PlayerState {
                        id: 0,
                        pos: feet.to_array(),
                        yaw: self.camera.yaw,
                        pitch: self.camera.pitch,
                        walk_phase: self.walk_phase,
                        is_moving: horiz_speed > 0.15,
                        arm_swing: self.arm_swing_progress,
                        held_block,
                    });
                }
            }
        }
 else if self.game_client.is_some() {
            let events = self.game_client.as_mut().unwrap().poll_events();
            let mut client_block_changed = false;
            let mut client_disconnected = false;

            for event in events {
                match event {
                    ClientEvent::Connected {
                        time_of_day,
                        edits,
                        ..
                    } => {
                        self.time_of_day = time_of_day;
                        for (pos, block_id) in edits {
                            self.world
                                .set_block(IVec3::from_array(pos), BlockId(block_id));
                        }
                        client_block_changed = true;
                        self.net_status = Some((
                            "Успешно подключено к серверу!".to_string(),
                            egui::Color32::GREEN,
                            Instant::now(),
                        ));
                        self.push_chat(
                            "[Сеть] Успешно подключено к серверу!",
                            egui::Color32::from_rgb(120, 220, 120),
                        );
                    }
                    ClientEvent::BlockChange { pos, block_id } => {
                        self.world
                            .set_block(IVec3::from_array(pos), BlockId(block_id));
                        client_block_changed = true;
                    }
                    ClientEvent::ChatMessage {
                        sender_name,
                        text,
                        ..
                    } => {
                        self.push_chat(format!("<{sender_name}> {text}"), egui::Color32::WHITE);
                    }
                    ClientEvent::PlayerJoined { name, .. } => {
                        self.push_chat(
                            format!("+ {} вошёл в игру", name),
                            egui::Color32::from_rgb(120, 220, 120),
                        );
                    }
                    ClientEvent::PlayerLeft { id } => {
                        self.push_chat(format!("- Игрок_{id} вышел из игры"), egui::Color32::GRAY);
                    }
                    ClientEvent::HitPlayer {
                        attacker_id,
                        target_id,
                        damage,
                        knockback,
                    } => {
                        let my_id = self.game_client.as_ref().map(|c| c.my_id).unwrap_or(u32::MAX);
                        if target_id == my_id {
                            self.health = (self.health - damage).max(0.0);
                            self.velocity = Vec3::new(self.velocity.x * 0.25, 0.0, self.velocity.z * 0.25) + Vec3::from_array(knockback);
                            self.last_damage_time = Some(Instant::now());
                            self.camera_flinch = 0.08;
                            let attacker_name = self.game_client.as_ref().and_then(|c| c.remote_players.get(&attacker_id)).map(|p| p.name.clone()).unwrap_or_else(|| format!("Игрок {attacker_id}"));
                            self.last_attacker_name = Some(attacker_name.clone());
                            self.last_damage_info = Some((attacker_name.clone(), damage, Instant::now()));
                            audio::play_sound(audio::SoundEffect::Hurt);
                            if let Some(client) = &mut self.game_client {
                                let _ = client.send_packet(&Packet::PlayerHealth { player_id: my_id, health: self.health });
                            }
                            if self.health <= 0.0 {
                                audio::play_sound(audio::SoundEffect::Death);
                                self.set_cursor_lock(false);
                                self.push_chat(format!("[PvP] {attacker_name} победил Вас!"), egui::Color32::from_rgb(255, 100, 100));
                            }
                        } else {
                            if let Some(client) = &mut self.game_client {
                                if let Some(target) = client.remote_players.get_mut(&target_id) {
                                    target.health = (target.health - damage).max(0.0);
                                    target.hurt_timer = 0.35;
                                }
                            }
                        }
                    }
                    ClientEvent::PlayerHealth { player_id, health } => {
                        if let Some(client) = &mut self.game_client {
                            if let Some(player) = client.remote_players.get_mut(&player_id) {
                                player.health = health;
                            }
                        }
                    }
                    ClientEvent::HostHandshakeReceived { server_name, experience_id, sandbox } => {
                        let pvp_str = if sandbox.pvp_enabled { "Вкл" } else { "Выкл" };
                        self.push_chat(
                            format!("[Сервер '{server_name}'] Режим: {experience_id}, PvP: {pvp_str}, Гравитация: {:.1}x", sandbox.gravity_multiplier),
                            egui::Color32::from_rgb(100, 220, 255),
                        );
                    }
                    ClientEvent::RulesUpdated(sandbox) => {
                        let pvp_str = if sandbox.pvp_enabled { "Вкл" } else { "Выкл" };
                        self.push_chat(
                            format!("[Правила сервера обновлены] PvP: {pvp_str}, Гравитация: {:.1}x", sandbox.gravity_multiplier),
                            egui::Color32::from_rgb(255, 200, 100),
                        );
                    }
                    ClientEvent::Teleported(pos) => {
                        self.set_player_feet(Vec3::from_array(pos));
                        self.velocity = Vec3::ZERO;
                        self.push_chat("[Хост] Вы были телепортированы хостом сервера!", egui::Color32::YELLOW);
                    }
                    ClientEvent::Disconnected => {
                        client_disconnected = true;
                        self.net_status = Some((
                            "Связь с сервером разорвана".to_string(),
                            egui::Color32::RED,
                            Instant::now(),
                        ));
                        self.push_chat("[Сеть] Отключено от сервера", egui::Color32::RED);
                    }
                    ClientEvent::DisconnectedWithReason(reason) => {
                        client_disconnected = true;
                        self.net_status = Some((
                            format!("Отключен от сервера: {reason}"),
                            egui::Color32::RED,
                            Instant::now(),
                        ));
                        self.push_chat(format!("[Сеть] Отключен: {reason}"), egui::Color32::RED);
                    }
                }
            }

            if client_disconnected {
                self.game_client = None;
            } else {
                if client_block_changed {
                    let local_meshes = self.world.rebuild_dirty_meshes(&self.registry, 8);
                    if !local_meshes.is_empty() {
                        if let Some(renderer) = &mut self.renderer {
                            renderer.upload_chunk_meshes(local_meshes);
                        }
                    }
                }

                if let Some(client) = &mut self.game_client {
                    client.update_remote_players(dt);
                }

                // Sync client player state to server at 25 Hz
                if self.last_net_sync.elapsed().as_millis() >= 40 {
                    self.last_net_sync = Instant::now();
                    let feet = self.player_feet();
                    let horiz_speed = Vec3::new(self.velocity.x, 0.0, self.velocity.z).length();
                    let held_block = self.inventory.selected_item().filter(|s| s.block_id != BlockId::AIR).map(|s| s.block_id.0).unwrap_or(0);
                    if let Some(client) = &mut self.game_client {
                        client.send_player_state(
                            feet,
                            self.camera.yaw,
                            self.camera.pitch,
                            self.walk_phase,
                            horiz_speed > 0.15,
                            self.arm_swing_progress,
                            held_block,
                        );
                    }
                }
            }
        }

    }

    fn handle_mouse_click(&mut self, button: MouseButton) {
        if self.screen != MenuScreen::Playing || self.chat_open || self.health <= 0.0 {
            return;
        }
        if !self.input.cursor_locked {
            self.set_cursor_lock(true);
            return;
        }

        self.arm_swing_active = true;
        if self.arm_swing_progress == 0.0 {
            self.arm_swing_progress = 0.05;
        }

        if button == MouseButton::Left {
            let can_attack = self
                .last_attack_time
                .map(|t| t.elapsed().as_secs_f32() >= 0.35)
                .unwrap_or(true);
            if can_attack {
                if let Some((target_id, target_name, _dist)) = self.find_pvp_target(3.8) {
                    self.last_attack_time = Some(Instant::now());

                    // Jump attack (Critical hit from a jump)
                    let is_crit = !self.on_ground && self.velocity.y < -0.05 && !self.fly_mode;
                    let base_damage = 5.0_f32;
                    let damage = if is_crit { 7.5_f32 } else { base_damage };

                    // Balanced knockback (небольшое отталкивание)
                    let fwd_xz = Vec3::new(self.camera.forward().x, 0.0, self.camera.forward().z).normalize_or_zero();
                    let kb_speed = if is_crit { 6.5 } else { 4.5 };
                    let kb_up = if is_crit { 3.2 } else { 2.4 };
                    let kb = (fwd_xz * kb_speed + Vec3::new(0.0, kb_up, 0.0)).to_array();

                    let my_id = if self.server_host.is_some() {
                        0
                    } else if let Some(client) = &self.game_client {
                        client.my_id
                    } else {
                        0
                    };

                    let hit_pkt = Packet::HitPlayer {
                        attacker_id: my_id,
                        target_id,
                        damage,
                        knockback: kb,
                    };

                    if let Some(host) = &mut self.server_host {
                        if let Some(victim) = self.host_remote_players.get_mut(&target_id) {
                            victim.health = (victim.health - damage).max(0.0);
                            victim.hurt_timer = 0.35;
                        }
                        host.broadcast(&hit_pkt);
                    } else if let Some(client) = &mut self.game_client {
                        if let Some(target) = client.remote_players.get_mut(&target_id) {
                            target.health = (target.health - damage).max(0.0);
                            target.hurt_timer = 0.35;
                        }
                        let _ = client.send_packet(&hit_pkt);
                    }

                    // Sounds & Hitmarker
                    if is_crit {
                        audio::play_sound(audio::SoundEffect::Crit);
                    } else {
                        audio::play_sound(audio::SoundEffect::Hit);
                    }
                    self.last_hitmarker = Some((Instant::now(), is_crit, damage));

                    let crit_text = if is_crit { " [КРИТ С ПРЫЖКА!]" } else { "" };
                    self.push_chat(
                        format!("[PvP] Удар по {target_name}{crit_text}! (-{:.1} HP)", damage),
                        if is_crit { egui::Color32::from_rgb(255, 215, 0) } else { egui::Color32::from_rgb(255, 140, 90) },
                    );
                    return;
                }
            }
        }


        let hit =
            self.world
                .raycast(self.camera.position, self.camera.forward(), 6.5, &self.registry);
        let Some(hit) = hit else {
            return;
        };

        let mut modified = false;
        match button {
            MouseButton::Left => {
                if self.player_mode == PlayerMode::Creative {
                    let block_name = self.registry.get(hit.block).map(|d| d.name.clone()).unwrap_or_default();
                    self.apply_and_broadcast_block(hit.hit_pos, BlockId::AIR);
                    audio::play_block_break(&block_name, Some(hit.hit_pos.as_vec3()));
                    modified = true;
                }
            }
            MouseButton::Right => {
                let place_id = self.selected_block();
                if place_id != BlockId::AIR
                    && (self.fly_mode
                        || !block_intersects_player(hit.place_pos, self.player_feet()))
                {
                    let can_place = if self.player_mode == PlayerMode::Creative {
                        true
                    } else {
                        self.inventory.consume_selected(1)
                    };
                    if can_place {
                        self.apply_and_broadcast_block(hit.place_pos, place_id);
                        audio::play_event_at("block.place", hit.place_pos.as_vec3());
                        modified = true;
                        self.arm_swing_active = true;
                        self.arm_swing_progress = 0.15;
                    }
                }
            }
            _ => {}
        }

        if modified {
            let local_meshes = self.world.rebuild_dirty_meshes(&self.registry, 8);
            if !local_meshes.is_empty() {
                if let Some(renderer) = &mut self.renderer {
                    renderer.upload_chunk_meshes(local_meshes);
                }
            }
        }

    }

    fn build_ui(&mut self, event_loop: &ActiveEventLoop) -> egui::FullOutput {
        let Some(window) = &self.window else {
            return egui::FullOutput::default();
        };
        let Some(egui_state) = &mut self.egui_state else {
            return egui::FullOutput::default();
        };

        let raw_input = egui_state.take_egui_input(window);
        let mut open_select_world_requested = false;
        let mut open_create_world_requested = false;
        let mut cancel_create_world_requested = false;
        let mut select_world_id_requested: Option<String> = None;
        let mut play_selected_world_requested = false;
        let mut delete_selected_world_requested = false;
        let mut back_to_title_from_select_requested = false;
        let mut create_and_start_world_requested = false;
        let mut back_to_title_requested = false;
        let mut resume_requested = false;
        let mut open_settings_requested = false;
        let open_multiplayer_requested = false;
        let mut open_host_server_requested = false;
        let mut open_join_server_requested = false;
        let mut start_host_session_requested = false;
        let mut save_host_profile_requested = false;
        let mut delete_host_profile_requested = false;
        let mut new_host_profile_requested = false;
        let mut kick_client_requested: Option<u32> = None;
        let mut teleport_client_to_host_requested: Option<u32> = None;
        let mut apply_live_rules_requested = false;
        let mut connect_to_server_requested: Option<(SocketAddr, Option<String>)> = None;
        let start_server_requested = false;
        let stop_server_requested = false;
        let mut connect_requested = false;
        let disconnect_client_requested = false;
        let mut back_to_pause_requested = false;
        let mut reset_settings_requested = false;
        let mut quit_requested = false;
        let mut settings_changed = false;
        let mut submit_chat_requested = false;
        let mut slot_action: Option<SlotAction> = None;
        let mut close_inventory_requested = false;
        let mut creative_pick_block: Option<BlockId> = None;
        let mut clear_inventory_requested = false;
        let mut respawn_requested = false;
        let tp_to_player_requested: Option<(String, Vec3)> = None;

        let _mode_id = self.mode.id();
        let player_mode = self.player_mode;
        let fly_mode = self.fly_mode;
        let fps = self.displayed_fps;
        let time_of_day = self.time_of_day;
        let _perspective_label = self.perspective.label_ru();
        let _chunk_count = self.world.chunk_count();
        let pos = self.camera.position;
        let selected_slot = self.inventory.selected_hotbar;
        let inventory_slots = self.inventory.slots;
        let break_progress = self.break_progress;
        let inventory_open = self.inventory_open;
        let held_stack = self.held_stack;
        let registry = Arc::clone(&self.registry);

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            // Minimalist HUD overlay (only in Playing mode)
            if self.settings.show_hud && self.screen == MenuScreen::Playing {
                egui::Area::new(egui::Id::new("hud_overlay"))
                    .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::NONE
                            .fill(egui::Color32::from_black_alpha(110))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                let fly_label = if fly_mode { " [Полёт]" } else { "" };
                                let hours = time_of_day.floor() as u32;
                                let minutes = ((time_of_day - hours as f32) * 60.0).floor() as u32;
                                ui.label(
                                    egui::RichText::new(format!(
                                        "FPS: {}   {:02}:{:02}   XYZ: {:.1}, {:.1}, {:.1}{}",
                                        fps, hours, minutes, pos.x, pos.y, pos.z, fly_label
                                    ))
                                    .color(egui::Color32::from_white_alpha(220))
                                    .size(12.0),
                                );
                            });
                    });
            }

            // Red Damage Screen Flash
            if let Some(dmg_time) = self.last_damage_time {
                let elapsed = dmg_time.elapsed().as_secs_f32();
                if elapsed < 0.35 {
                    let alpha = ((1.0 - elapsed / 0.35) * 80.0) as u8;
                    let painter = ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::new("damage_flash"),
                    ));
                    painter.rect_filled(
                        ctx.screen_rect(),
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(220, 20, 20, alpha),
                    );
                }
            }

            // Damage banner notification (who attacked me)
            if self.screen == MenuScreen::Playing && self.health > 0.0 {
                if let Some((attacker, dmg, time)) = &self.last_damage_info {
                    let elapsed = time.elapsed().as_secs_f32();
                    if elapsed < 1.8 {
                        let alpha = (((1.8 - elapsed) / 1.8) * 255.0) as u8;
                        egui::Area::new(egui::Id::new("damage_banner"))
                            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -96.0))
                            .interactable(false)
                            .show(ctx, |ui| {
                                egui::Frame::NONE
                                    .fill(egui::Color32::from_rgba_unmultiplied(35, 10, 10, alpha))
                                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgba_unmultiplied(220, 50, 50, alpha)))
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::symmetric(14, 5))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("⚔ Вас атаковал {} (-{:.1} HP)", attacker, dmg))
                                                .size(13.0)
                                                .strong()
                                                .color(egui::Color32::from_rgba_unmultiplied(255, 90, 90, alpha)),
                                        );
                                    });
                            });
                    }
                }
            }

            // Local player health bar (in Playing mode)
            if self.screen == MenuScreen::Playing && self.health > 0.0 {
                egui::Area::new(egui::Id::new("local_health_bar"))
                    .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -68.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::NONE
                            .fill(egui::Color32::from_black_alpha(150))
                            .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(35)))
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let hp_ratio = (self.health / 100.0).clamp(0.0, 1.0);
                                    let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 8.0), egui::Sense::hover());
                                    ui.painter().rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(45, 15, 15));
                                    let fill_w = bar_rect.width() * hp_ratio;
                                    if fill_w > 0.0 {
                                        let col = if hp_ratio > 0.5 {
                                            egui::Color32::from_rgb(220, 45, 45)
                                        } else {
                                            egui::Color32::from_rgb(255, 30, 30)
                                        };
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, bar_rect.height())),
                                            2.0,
                                            col,
                                        );
                                    }
                                    ui.label(
                                        egui::RichText::new(format!("{:.0} HP", self.health))
                                            .size(11.0)
                                            .strong()
                                            .color(egui::Color32::WHITE),
                                    );
                                });
                            });
                    });
            }

            // Death Screen Overlay
            if self.health <= 0.0 && self.screen == MenuScreen::Playing {
                egui::Window::new("DeathScreen")
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .frame(
                        egui::Frame::NONE
                            .fill(egui::Color32::from_black_alpha(225))
                            .stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(220, 40, 40)))
                            .corner_radius(10.0)
                            .inner_margin(egui::Margin::symmetric(36, 26)),
                    )
                    .fixed_size(egui::vec2(320.0, 175.0))
                    .show(ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("ВЫ ПОГИБЛИ")
                                    .size(26.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(255, 60, 60)),
                            );
                            ui.add_space(8.0);
                            let killer_text = if let Some(killer) = &self.last_attacker_name {
                                format!("Вас убил: {}", killer)
                            } else {
                                "Вы погибли".to_string()
                            };
                            ui.label(
                                egui::RichText::new(killer_text)
                                    .size(14.5)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            ui.add_space(18.0);
                            if ui
                                .add_sized([200.0, 38.0], egui::Button::new(
                                    egui::RichText::new("Возродиться").size(15.0).strong()
                                ))
                                .clicked()
                            {
                                respawn_requested = true;
                            }
                            ui.add_space(4.0);
                        });
                    });
            }

            // Crosshair in Playing screen
            if self.screen == MenuScreen::Playing && !inventory_open && self.health > 0.0 {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("crosshair"),
                ));
                let center = ctx.screen_rect().center();
                let stroke = egui::Stroke::new(2.0_f32, egui::Color32::from_white_alpha(220));
                painter.line_segment(
                    [center - egui::vec2(8.0, 0.0), center + egui::vec2(8.0, 0.0)],
                    stroke,
                );
                painter.line_segment(
                    [center - egui::vec2(0.0, 8.0), center + egui::vec2(0.0, 8.0)],
                    stroke,
                );

                // Hitmarker (✕) on hitting an enemy
                if let Some((hit_time, is_crit, dmg)) = self.last_hitmarker {
                    let elapsed = hit_time.elapsed().as_secs_f32();
                    if elapsed < 0.28 {
                        let alpha = (((0.28 - elapsed) / 0.28) * 255.0) as u8;
                        let hm_col = if is_crit {
                            egui::Color32::from_rgba_unmultiplied(255, 215, 0, alpha)
                        } else {
                            egui::Color32::from_rgba_unmultiplied(255, 60, 60, alpha)
                        };
                        let hm_stroke = egui::Stroke::new(if is_crit { 2.4_f32 } else { 1.8_f32 }, hm_col);
                        let s = if is_crit { 9.0 } else { 7.0 };
                        let d = 3.5;
                        painter.line_segment([center + egui::vec2(d, d), center + egui::vec2(s, s)], hm_stroke);
                        painter.line_segment([center + egui::vec2(-d, d), center + egui::vec2(-s, s)], hm_stroke);
                        painter.line_segment([center + egui::vec2(d, -d), center + egui::vec2(s, -s)], hm_stroke);
                        painter.line_segment([center + egui::vec2(-d, -d), center + egui::vec2(-s, -s)], hm_stroke);

                        let dmg_text = if is_crit {
                            format!("-{:.1} КРИТ!", dmg)
                        } else {
                            format!("-{:.1}", dmg)
                        };
                        painter.text(
                            center + egui::vec2(16.0, -14.0),
                            egui::Align2::LEFT_BOTTOM,
                            dmg_text,
                            egui::FontId::proportional(12.5),
                            hm_col,
                        );
                    }
                }

                // Mining crack progress indicator
                if break_progress > 0.01 {
                    let p = break_progress.clamp(0.0, 1.0);
                    let radius = 15.0_f32;
                    let n_pts = (32.0 * p).ceil() as usize;
                    if n_pts >= 2 {
                        let pts: Vec<egui::Pos2> = (0..=n_pts)
                            .map(|i| {
                                let angle = (i as f32 / 32.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                                center + egui::vec2(angle.cos() * radius, angle.sin() * radius)
                            })
                            .collect();
                        for w in pts.windows(2) {
                            painter.line_segment([w[0], w[1]], egui::Stroke::new(2.5_f32, egui::Color32::from_rgb(255, 185, 40)));
                        }
                    }
                    let bar_rect = egui::Rect::from_center_size(center + egui::vec2(0.0, 24.0), egui::vec2(36.0, 4.0));
                    painter.rect_filled(bar_rect, 2.0, egui::Color32::from_black_alpha(160));
                    let fill_rect = egui::Rect::from_min_size(bar_rect.min, egui::vec2(36.0 * p, 4.0));
                    painter.rect_filled(fill_rect, 2.0, egui::Color32::from_rgb(255, 195, 50));
                }

                // Bottom Hotbar (Quick slot block selector)
                egui::Area::new(egui::Id::new("hotbar_area"))
                    .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -14.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            // Selected block name floating above the hotbar
                            if let Some(stack) = inventory_slots[selected_slot] {
                                if let Some(def) = registry.get(stack.block_id) {
                                    let name = def.name.split(':').last().unwrap_or(&def.name);
                                    egui::Frame::NONE
                                        .fill(egui::Color32::from_black_alpha(175))
                                        .corner_radius(4.0)
                                        .inner_margin(egui::Margin::symmetric(10, 3))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(name)
                                                    .color(egui::Color32::WHITE)
                                                    .size(13.0)
                                                    .strong(),
                                            );
                                        });
                                    ui.add_space(4.0);
                                }
                            }

                            egui::Frame::NONE
                                .fill(egui::Color32::from_black_alpha(165))
                                .stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_white_alpha(45)))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = egui::vec2(5.0, 0.0);
                                        for idx in 0..9 {
                                            let is_sel = idx == selected_slot;
                                            render_hud_hotbar_slot(
                                                ui,
                                                idx,
                                                inventory_slots[idx],
                                                is_sel,
                                                &registry,
                                            );
                                        }
                                    });
                                });
                        });
                    });
            }

            // Floating nametags for remote players
            let remote_players: Vec<RemotePlayer> = if self.server_host.is_some() {
                self.host_remote_players.values().cloned().collect()
            } else if let Some(client) = &self.game_client {
                client.remote_players.values().cloned().collect()
            } else {
                Vec::new()
            };

            if !remote_players.is_empty() {
                let screen_rect = ctx.screen_rect();
                let aspect = screen_rect.width() / screen_rect.height().max(1.0);
                let vp = self.camera.view_proj(aspect);
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("nametags"),
                ));

                for player in &remote_players {
                    let head_pos = player.current_pos + Vec3::new(0.0, 1.95, 0.0);
                    let clip = vp * head_pos.extend(1.0);
                    if clip.w > 0.1 && clip.z > 0.0 {
                        let ndc = clip.truncate() / clip.w;
                        if ndc.x >= -1.05 && ndc.x <= 1.05 && ndc.y >= -1.05 && ndc.y <= 1.05 {
                            let screen_x = (ndc.x * 0.5 + 0.5) * screen_rect.width();
                            let screen_y = (1.0 - (ndc.y * 0.5 + 0.5)) * screen_rect.height();

                            let font_id = egui::FontId::proportional(12.0);
                            let text = &player.name;
                            let text_galley =
                                painter.layout_no_wrap(text.clone(), font_id, egui::Color32::WHITE);
                            let badge_w = text_galley.size().x.max(46.0) + 14.0;
                            let badge_h = text_galley.size().y + 10.0;
                            let badge_rect = egui::Rect::from_center_size(
                                egui::pos2(screen_x, screen_y),
                                egui::vec2(badge_w, badge_h),
                            );
                            painter.rect_filled(badge_rect, 4.0, egui::Color32::from_black_alpha(150));
                            painter.galley(
                                badge_rect.min + egui::vec2(7.0, 2.0),
                                text_galley,
                                egui::Color32::WHITE,
                            );

                            // Mini health bar under remote player name
                            let hp_ratio = (player.health / 100.0).clamp(0.0, 1.0);
                            let bar_w = badge_rect.width() - 14.0;
                            let bar_rect = egui::Rect::from_min_size(
                                badge_rect.min + egui::vec2(7.0, badge_rect.height() - 4.5),
                                egui::vec2(bar_w, 2.5),
                            );
                            painter.rect_filled(bar_rect, 1.0, egui::Color32::from_black_alpha(180));
                            let fill_w = bar_w * hp_ratio;
                            if fill_w > 0.0 {
                                let bar_col = if hp_ratio > 0.5 {
                                    egui::Color32::from_rgb(80, 220, 80)
                                } else if hp_ratio > 0.25 {
                                    egui::Color32::from_rgb(230, 200, 50)
                                } else {
                                    egui::Color32::from_rgb(230, 50, 50)
                                };
                                painter.rect_filled(
                                    egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, 2.5)),
                                    1.0,
                                    bar_col,
                                );
                            }
                        }
                    }
                }
            }

            // Chat area (bottom-left)
            let now = Instant::now();
            let recent_messages: Vec<&ChatEntry> = self
                .chat_history
                .iter()
                .filter(|m| self.chat_open || now.duration_since(m.timestamp).as_secs_f32() < 8.0)
                .collect();

            if !recent_messages.is_empty() || self.chat_open {
                egui::Area::new(egui::Id::new("chat_area"))
                    .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -52.0))
                    .show(ctx, |ui| {
                        let bg = if self.chat_open {
                            egui::Color32::from_black_alpha(170)
                        } else {
                            egui::Color32::from_black_alpha(90)
                        };

                        egui::Frame::NONE
                            .fill(bg)
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.set_max_width(460.0);
                                egui::ScrollArea::vertical()
                                    .max_height(if self.chat_open { 210.0 } else { 140.0 })
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        for entry in recent_messages.iter().rev().take(12).rev() {
                                            let alpha = if self.chat_open {
                                                255
                                            } else {
                                                let age = now
                                                    .duration_since(entry.timestamp)
                                                    .as_secs_f32();
                                                if age > 6.0 {
                                                    ((8.0 - age) / 2.0 * 255.0) as u8
                                                } else {
                                                    255
                                                }
                                            };
                                            let color = egui::Color32::from_rgba_unmultiplied(
                                                entry.color.r(),
                                                entry.color.g(),
                                                entry.color.b(),
                                                alpha,
                                            );
                                            ui.label(
                                                egui::RichText::new(&entry.text)
                                                    .color(color)
                                                    .size(12.5),
                                            );
                                        }
                                    });

                                if self.chat_open {
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        let output = egui::TextEdit::singleline(&mut self.chat_input)
                                            .hint_text("Сообщение или /команда...")
                                            .desired_width(420.0)
                                            .show(ui);
                                        if self.chat_focus_requested {
                                            output.response.request_focus();
                                            let mut state = output.state;
                                            let ccursor = egui::text::CCursor::new(
                                                self.chat_input.chars().count(),
                                            );
                                            state.cursor.set_char_range(Some(
                                                egui::text::CCursorRange::one(ccursor),
                                            ));
                                            state.store(ctx, output.response.id);
                                            self.chat_focus_requested = false;
                                        }
                                        if output.response.lost_focus()
                                            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                                        {
                                            submit_chat_requested = true;
                                        }
                                    });
                                }
                            });
                    });
            }

            // Dim background behind pause/settings menu (leave title screen bright to show 3D panorama)
            if self.screen != MenuScreen::Playing && self.screen != MenuScreen::Title {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Background,
                    egui::Id::new("menu_dim"),
                ));
                painter.rect_filled(
                    ctx.screen_rect(),
                    0.0,
                    egui::Color32::from_black_alpha(145),
                );
            }

            match self.screen {
                MenuScreen::Playing => {}
                MenuScreen::Title => {
                    egui::Window::new("TitleScreen")
                        .collapsible(false)
                        .resizable(false)
                        .title_bar(false)
                        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                        .frame(
                            egui::Frame::NONE
                                .fill(egui::Color32::from_black_alpha(170))
                                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(35)))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::symmetric(36, 26)),
                        )
                        .show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new("VOXEL ENGINE")
                                        .size(26.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.label(
                                    egui::RichText::new("Voxel Sandbox & Multiplayer")
                                        .size(11.5)
                                        .color(egui::Color32::from_white_alpha(140)),
                                );
                                ui.add_space(20.0);

                                if ui
                                    .add_sized([240.0, 36.0], egui::Button::new(
                                        egui::RichText::new("Одиночная игра (Solo)").size(14.0)
                                    ))
                                    .clicked()
                                {
                                    open_select_world_requested = true;
                                }
                                ui.add_space(8.0);

                                if ui
                                    .add_sized([240.0, 36.0], egui::Button::new(
                                        egui::RichText::new("Создать сервер (Host Co-op)").size(14.0)
                                    ))
                                    .clicked()
                                {
                                    open_host_server_requested = true;
                                }
                                ui.add_space(8.0);

                                if ui
                                    .add_sized([240.0, 36.0], egui::Button::new(
                                        egui::RichText::new("Сетевая игра (Join Server)").size(14.0)
                                    ))
                                    .clicked()
                                {
                                    open_join_server_requested = true;
                                }
                                ui.add_space(8.0);

                                if ui
                                    .add_sized([240.0, 36.0], egui::Button::new(
                                        egui::RichText::new("Настройки").size(14.0)
                                    ))
                                    .clicked()
                                {
                                    open_settings_requested = true;
                                }
                                ui.add_space(8.0);

                                if ui
                                    .add_sized([240.0, 36.0], egui::Button::new(
                                        egui::RichText::new("Выход").size(14.0)
                                    ))
                                    .clicked()
                                {
                                    quit_requested = true;
                                }
                                ui.add_space(4.0);
                            });
                        });
                }
                MenuScreen::SelectWorld => {
                    egui::Window::new("SelectWorldScreen")
                        .collapsible(false)
                        .resizable(false)
                        .title_bar(false)
                        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                        .fixed_size([540.0, 440.0])
                        .frame(
                            egui::Frame::NONE
                                .fill(egui::Color32::from_black_alpha(190))
                                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(40)))
                                .corner_radius(10.0)
                                .inner_margin(egui::Margin::symmetric(24, 20)),
                        )
                        .show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("ВЫБОР МИРА")
                                        .size(20.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.label(
                                    egui::RichText::new("Выберите мир для загрузки или создайте новый")
                                        .size(11.5)
                                        .color(egui::Color32::from_white_alpha(150)),
                                );
                                ui.add_space(12.0);

                                egui::ScrollArea::vertical()
                                    .max_height(260.0)
                                    .show(ui, |ui| {
                                        if self.cached_world_list.is_empty() {
                                            ui.add_space(40.0);
                                            ui.label(
                                                egui::RichText::new("Нет созданных миров.\nСоздайте новый мир, нажав кнопку ниже!")
                                                    .size(13.0)
                                                    .color(egui::Color32::LIGHT_GRAY),
                                            );
                                            ui.add_space(40.0);
                                        } else {
                                            for world in &self.cached_world_list {
                                                let is_selected = self.selected_world_id.as_ref() == Some(&world.id);
                                                let bg_color = if is_selected {
                                                    egui::Color32::from_rgba_premultiplied(45, 75, 120, 200)
                                                } else {
                                                    egui::Color32::from_black_alpha(85)
                                                };
                                                let border_color = if is_selected {
                                                    egui::Color32::from_rgb(110, 190, 255)
                                                } else {
                                                    egui::Color32::from_white_alpha(20)
                                                };

                                                let frame = egui::Frame::NONE
                                                    .fill(bg_color)
                                                    .stroke(egui::Stroke::new(1.0_f32, border_color))
                                                    .corner_radius(6.0)
                                                    .inner_margin(egui::Margin::symmetric(14, 10));

                                                frame.show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.vertical(|ui| {
                                                            ui.label(
                                                                egui::RichText::new(&world.name)
                                                                    .size(15.0)
                                                                    .strong()
                                                                    .color(if is_selected { egui::Color32::WHITE } else { egui::Color32::from_gray(225) }),
                                                            );
                                                            let mode_label = if world.mode == "Creative" { "Креатив" } else { "Выживание" };
                                                            let gen_label = if world.generator == "Classic" { "Классика" } else { "Литосфера" };
                                                            ui.label(
                                                                egui::RichText::new(format!(
                                                                    "{mode_label} • {gen_label} • Сид: {} • {}",
                                                                    world.seed, world.last_played
                                                                ))
                                                                .size(11.0)
                                                                .color(egui::Color32::from_gray(160)),
                                                            );
                                                        });
                                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                            if !is_selected {
                                                                if ui.button("Выбрать").clicked() {
                                                                    select_world_id_requested = Some(world.id.clone());
                                                                }
                                                            } else {
                                                                ui.label(egui::RichText::new("✓ Выбран").color(egui::Color32::LIGHT_GREEN).strong());
                                                            }
                                                        });
                                                    });
                                                });
                                                ui.add_space(5.0);
                                            }
                                        }
                                    });

                                ui.add_space(14.0);

                                ui.horizontal(|ui| {
                                    let has_selection = self.selected_world_id.is_some() && !self.cached_world_list.is_empty();
                                    if ui.add_enabled(has_selection, egui::Button::new(
                                        egui::RichText::new("Играть в мире").strong()
                                    ).min_size(egui::vec2(160.0, 34.0))).clicked() {
                                        play_selected_world_requested = true;
                                    }

                                    if ui.add_sized([150.0, 34.0], egui::Button::new("Создать мир")).clicked() {
                                        open_create_world_requested = true;
                                    }

                                    if ui.add_enabled(has_selection, egui::Button::new(
                                        egui::RichText::new("Удалить").color(egui::Color32::from_rgb(255, 120, 120))
                                    ).min_size(egui::vec2(80.0, 34.0))).clicked() {
                                        delete_selected_world_requested = true;
                                    }

                                    if ui.add_sized([80.0, 34.0], egui::Button::new("Назад")).clicked() {
                                        back_to_title_from_select_requested = true;
                                    }
                                });
                            });
                        });
                }
                MenuScreen::CreateWorld => {
                    egui::Window::new("CreateWorldScreen")
                        .collapsible(false)
                        .resizable(false)
                        .title_bar(false)
                        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                        .fixed_size([480.0, 410.0])
                        .frame(
                            egui::Frame::NONE
                                .fill(egui::Color32::from_black_alpha(195))
                                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(40)))
                                .corner_radius(10.0)
                                .inner_margin(egui::Margin::symmetric(28, 22)),
                        )
                        .show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("СОЗДАНИЕ НОВОГО МИРА")
                                        .size(20.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.add_space(14.0);
                            });

                            ui.label(egui::RichText::new("Название мира:").size(12.5).strong().color(egui::Color32::WHITE));
                            ui.add(egui::TextEdit::singleline(&mut self.create_world_name).desired_width(f32::INFINITY));
                            ui.add_space(10.0);

                            ui.label(egui::RichText::new("Сид мира (число или оставьте для генерации):").size(12.5).strong().color(egui::Color32::WHITE));
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut self.create_world_seed).desired_width(310.0));
                                if ui.button("Случайный").clicked() {
                                    let mut rng = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u32;
                                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                                    self.create_world_seed = format!("{}", rng % 1_000_000);
                                }
                            });
                            ui.add_space(12.0);

                            ui.label(egui::RichText::new("Режим игры:").size(12.5).strong().color(egui::Color32::WHITE));
                            ui.horizontal(|ui| {
                                let is_surv = self.create_world_mode == "Survival";
                                if ui.selectable_label(is_surv, "Выживание (Survival)").clicked() {
                                    self.create_world_mode = "Survival".to_string();
                                }
                                let is_creat = self.create_world_mode == "Creative";
                                if ui.selectable_label(is_creat, "Креатив (Creative)").clicked() {
                                    self.create_world_mode = "Creative".to_string();
                                }
                            });
                            ui.add_space(12.0);

                            ui.label(egui::RichText::new("Тип генератора:").size(12.5).strong().color(egui::Color32::WHITE));
                            ui.horizontal(|ui| {
                                let is_litho = self.create_world_gen == "Lithosphere";
                                if ui.selectable_label(is_litho, "Литосфера (Lithosphere)").clicked() {
                                    self.create_world_gen = "Lithosphere".to_string();
                                }
                                let is_classic = self.create_world_gen == "Classic";
                                if ui.selectable_label(is_classic, "Классический").clicked() {
                                    self.create_world_gen = "Classic".to_string();
                                }
                            });
                            ui.label(
                                egui::RichText::new("• Литосфера: реалистичные хребты, каньоны, пещеры и глубокие недра\n• Классический: холмистый рельеф с дубовыми деревьями")
                                    .size(10.5)
                                    .color(egui::Color32::from_white_alpha(130)),
                            );
                            ui.add_space(18.0);

                            ui.horizontal(|ui| {
                                if ui.add_sized([220.0, 36.0], egui::Button::new(
                                    egui::RichText::new("Создать и играть").strong().color(egui::Color32::WHITE)
                                )).clicked() {
                                    create_and_start_world_requested = true;
                                }
                                if ui.add_sized([190.0, 36.0], egui::Button::new("Отмена")).clicked() {
                                    cancel_create_world_requested = true;
                                }
                            });
                        });
                }
                MenuScreen::Pause => {
                    let title_text = match self.session_type {
                        SessionType::Solo => "ПАУЗА (ОДИНОЧНАЯ ИГРА)",
                        SessionType::Host => "УПРАВЛЕНИЕ СЕРВЕРОМ (ХОСТ)",
                        SessionType::Client => "МЕНЮ СЕТЕВОЙ ИГРЫ",
                    };
                    let fixed_width = if self.session_type == SessionType::Host { 380.0 } else { 280.0 };

                    egui::Window::new("Пауза")
                        .collapsible(false)
                        .resizable(false)
                        .title_bar(false)
                        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                        .frame(
                            egui::Frame::NONE
                                .fill(egui::Color32::from_black_alpha(185))
                                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(40)))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::symmetric(24, 18)),
                        )
                        .fixed_size(egui::vec2(fixed_width, 0.0))
                        .show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(2.0);
                                ui.label(
                                    egui::RichText::new(title_text)
                                        .size(17.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                if self.session_type == SessionType::Host {
                                    ui.label(
                                        egui::RichText::new("Сервер активен в реальном времени (мир не заморожен)")
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(140, 220, 140)),
                                    );
                                }
                                ui.add_space(14.0);

                                if ui
                                    .add_sized([220.0, 34.0], egui::Button::new("Вернуться в игру"))
                                    .clicked()
                                {
                                    resume_requested = true;
                                }
                                ui.add_space(8.0);

                                if self.session_type == SessionType::Host {
                                    ui.separator();
                                    ui.label(egui::RichText::new("Подключенные клиенты:").size(12.0).strong().color(egui::Color32::WHITE));
                                    ui.add_space(4.0);

                                    let mut has_clients = false;
                                    if let Some(host) = &self.server_host {
                                        for client in host.clients() {
                                            has_clients = true;
                                            ui.horizontal(|ui| {
                                                ui.label(format!("• {} (#{})", client.name, client.id));
                                                if ui.small_button("ТП к хосту").clicked() {
                                                    teleport_client_to_host_requested = Some(client.id);
                                                }
                                                if ui.small_button("Кикнуть").clicked() {
                                                    kick_client_requested = Some(client.id);
                                                }
                                            });
                                        }
                                    }
                                    if !has_clients {
                                        ui.label(egui::RichText::new("Нет других игроков на сервере").size(11.0).color(egui::Color32::GRAY));
                                    }

                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.label(egui::RichText::new("Настройки песочницы на лету:").size(12.0).strong().color(egui::Color32::WHITE));
                                    ui.add_space(4.0);

                                    ui.checkbox(&mut self.active_host_profile.sandbox.pvp_enabled, "Включить PvP (урон игрокам)");
                                    ui.checkbox(&mut self.active_host_profile.sandbox.structural_integrity, "Сопромат построек");
                                    ui.add(egui::Slider::new(&mut self.active_host_profile.sandbox.gravity_multiplier, 0.1..=3.0).text("Гравитация"));

                                    if ui.add_sized([220.0, 28.0], egui::Button::new("Применить правила для всех")).clicked() {
                                        apply_live_rules_requested = true;
                                    }
                                    ui.add_space(8.0);
                                    ui.separator();
                                }

                                if ui
                                    .add_sized([220.0, 32.0], egui::Button::new("Настройки"))
                                    .clicked()
                                {
                                    open_settings_requested = true;
                                }
                                ui.add_space(8.0);

                                let exit_label = match self.session_type {
                                    SessionType::Solo => "Сохранить и выйти в меню",
                                    SessionType::Host => "Остановить сервер и выйти",
                                    SessionType::Client => "Отключиться от сервера",
                                };

                                if ui
                                    .add_sized([220.0, 32.0], egui::Button::new(exit_label))
                                    .clicked()
                                {
                                    back_to_title_requested = true;
                                }
                            });
                        });
                }
                MenuScreen::Settings => {
                    egui::Window::new("Настройки")
                        .collapsible(false)
                        .resizable(false)
                        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                        .fixed_size(egui::vec2(490.0, 520.0))
                        .show(ctx, |ui| {
                            egui::ScrollArea::vertical().max_height(450.0).show(ui, |ui| {
                                ui.heading("Камера и мышь");
                                ui.add_space(4.0);

                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.settings.mouse_sensitivity,
                                            0.1..=3.0,
                                        )
                                        .text("Чувствительность мыши"),
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.settings.mouse_smoothing,
                                            0.0..=0.95,
                                        )
                                        .text("Сглаживание поворота (0 = выкл)"),
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                if ui
                                    .checkbox(&mut self.settings.invert_y, "Инверсия мыши по Y")
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                ui.add_space(10.0);
                                ui.separator();
                                ui.heading("Графика и мир");
                                ui.add_space(4.0);

                                if ui
                                    .checkbox(
                                        &mut self.settings.vsync,
                                        "VSync (вертикальная синхронизация)",
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.settings.fov_degrees,
                                            50.0..=110.0,
                                        )
                                        .text("Угол обзора (FOV)"),
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.render_distance, 2..=128)
                                            .text("Дальность прорисовки (чанки)"),
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.brightness, 0.4..=1.6)
                                            .text("Яркость мира"),
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.fly_speed, 4.0..=40.0)
                                            .text("Скорость полёта"),
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                if ui
                                    .checkbox(&mut self.settings.show_hud, "Показывать HUD (FPS/XYZ)")
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                ui.add_space(10.0);
                                ui.separator();
                                ui.heading("Звук и аудио");
                                ui.add_space(4.0);

                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.master_volume, 0.0..=1.0)
                                            .text("Общая громкость")
                                            .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
                                    )
                                    .changed()
                                {
                                    audio::set_volume(audio::SoundCategory::Master, self.settings.master_volume);
                                    settings_changed = true;
                                }

                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.sfx_volume, 0.0..=1.0)
                                            .text("Эффекты и блоки (SFX)")
                                            .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
                                    )
                                    .changed()
                                {
                                    audio::set_volume(audio::SoundCategory::Sfx, self.settings.sfx_volume);
                                    settings_changed = true;
                                }

                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.ambient_volume, 0.0..=1.0)
                                            .text("Окружение (Эмбиент)")
                                            .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
                                    )
                                    .changed()
                                {
                                    audio::set_volume(audio::SoundCategory::Ambient, self.settings.ambient_volume);
                                    settings_changed = true;
                                }

                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.settings.ui_volume, 0.0..=1.0)
                                            .text("Интерфейс (UI)")
                                            .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
                                    )
                                    .changed()
                                {
                                    audio::set_volume(audio::SoundCategory::Ui, self.settings.ui_volume);
                                    settings_changed = true;
                                }

                                ui.add_space(10.0);
                                ui.separator();
                                ui.heading("Клавиши управления");
                                ui.add_space(4.0);

                                let rows = [
                                    ("Вперёд", RebindTarget::Forward, self.settings.keybinds.forward.clone()),
                                    ("Назад", RebindTarget::Backward, self.settings.keybinds.backward.clone()),
                                    ("Влево", RebindTarget::Left, self.settings.keybinds.left.clone()),
                                    ("Вправо", RebindTarget::Right, self.settings.keybinds.right.clone()),
                                    ("Прыжок / Вверх", RebindTarget::JumpUp, self.settings.keybinds.jump_up.clone()),
                                    ("Вниз (в полёте)", RebindTarget::Down, self.settings.keybinds.down.clone()),
                                    ("Ускорение", RebindTarget::Sprint, self.settings.keybinds.sprint.clone()),
                                    ("Режим полёта", RebindTarget::ToggleFly, self.settings.keybinds.toggle_fly.clone()),
                                    ("Чат", RebindTarget::Chat, self.settings.keybinds.chat.clone()),
                                    ("Команда (/)", RebindTarget::Command, self.settings.keybinds.command.clone()),
                                ];

                                egui::Grid::new("keybinds_grid")
                                    .num_columns(2)
                                    .spacing([24.0, 6.0])
                                    .show(ui, |ui| {
                                        for (label, target, current_key) in rows {
                                            ui.label(label);
                                            let is_waiting = self.rebinding == Some(target);
                                            let btn_text = if is_waiting {
                                                "Нажмите клавишу...".to_string()
                                            } else {
                                                current_key
                                            };
                                            if ui
                                                .add_sized([160.0, 24.0], egui::Button::new(btn_text))
                                                .clicked()
                                            {
                                                self.rebinding = Some(target);
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });

                            ui.add_space(10.0);
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button("Сбросить по умолчанию").clicked() {
                                    reset_settings_requested = true;
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add_sized([120.0, 30.0], egui::Button::new("Назад")).clicked() {
                                        back_to_pause_requested = true;
                                    }
                                });
                            });
                        });
                }
                MenuScreen::HostServer => {
                    egui::Window::new("HostServerScreen")
                        .collapsible(false)
                        .resizable(false)
                        .title_bar(false)
                        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                        .frame(
                            egui::Frame::NONE
                                .fill(egui::Color32::from_black_alpha(195))
                                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(45)))
                                .corner_radius(10.0)
                                .inner_margin(egui::Margin::symmetric(28, 22)),
                        )
                        .fixed_size(egui::vec2(520.0, 480.0))
                        .show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("СОЗДАТЬ СЕРВЕР (HOST CO-OP)")
                                        .size(19.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.label(
                                    egui::RichText::new("Песочница и выделенный сервер для совместной игры")
                                        .size(11.0)
                                        .color(egui::Color32::from_white_alpha(140)),
                                );
                                ui.add_space(10.0);
                            });

                            // Profile selection & management row
                            ui.horizontal(|ui| {
                                ui.label("Профиль сервера:");
                                let cur_name = self.host_profile_name_input.clone();
                                egui::ComboBox::from_id_salt("host_profile_combo")
                                    .selected_text(&cur_name)
                                    .show_ui(ui, |ui| {
                                        for (i, (p_name, _)) in self.server_profiles.iter().enumerate() {
                                            if ui.selectable_label(p_name == &cur_name, p_name).clicked() {
                                                self.selected_server_profile_idx = i;
                                                self.host_profile_name_input = p_name.clone();
                                                if let Some(prof) = WorldManager::get_server_profile(p_name) {
                                                    self.active_host_profile = prof;
                                                }
                                            }
                                        }
                                    });

                                if ui.button("Новый").clicked() {
                                    new_host_profile_requested = true;
                                }
                                if ui.button("Сохранить").clicked() {
                                    save_host_profile_requested = true;
                                }
                                if self.server_profiles.len() > 1 && ui.button("Удалить").clicked() {
                                    delete_host_profile_requested = true;
                                }
                            });

                            ui.add_space(8.0);
                            ui.separator();

                            // Tabs: Network, Sandbox, World
                            ui.horizontal(|ui| {
                                if ui.selectable_label(self.host_active_tab == HostConfigTab::Network, "1. Сеть (Network)").clicked() {
                                    self.host_active_tab = HostConfigTab::Network;
                                }
                                if ui.selectable_label(self.host_active_tab == HostConfigTab::Sandbox, "2. Песочница (Sandbox)").clicked() {
                                    self.host_active_tab = HostConfigTab::Sandbox;
                                }
                                if ui.selectable_label(self.host_active_tab == HostConfigTab::World, "3. Мир (World)").clicked() {
                                    self.host_active_tab = HostConfigTab::World;
                                }
                            });
                            ui.separator();
                            ui.add_space(6.0);

                            match self.host_active_tab {
                                HostConfigTab::Network => {
                                    ui.horizontal(|ui| {
                                        ui.label("Имя сервера:");
                                        ui.add_sized([300.0, 22.0], egui::TextEdit::singleline(&mut self.active_host_profile.server_name));
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Порт сервера:");
                                        let mut port_str = self.active_host_profile.port.to_string();
                                        if ui.add_sized([90.0, 22.0], egui::TextEdit::singleline(&mut port_str)).changed() {
                                            if let Ok(p) = port_str.parse() {
                                                self.active_host_profile.port = p;
                                            }
                                        }
                                        ui.weak("(по умолчанию: 25565)");
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Максимум игроков:");
                                        let mut max_p = self.active_host_profile.max_players as u32;
                                        if ui.add(egui::Slider::new(&mut max_p, 2..=32)).changed() {
                                            self.active_host_profile.max_players = max_p as u16;
                                        }
                                    });
                                    ui.add_space(6.0);
                                    let mut has_pw = self.active_host_profile.password.is_some();
                                    if ui.checkbox(&mut has_pw, "Защитить паролем").changed() {
                                        if has_pw {
                                            self.active_host_profile.password = Some("1234".to_string());
                                        } else {
                                            self.active_host_profile.password = None;
                                        }
                                    }
                                    if let Some(pw) = &mut self.active_host_profile.password {
                                        ui.horizontal(|ui| {
                                            ui.label("Пароль:");
                                            ui.add_sized([180.0, 22.0], egui::TextEdit::singleline(pw));
                                        });
                                    }
                                    ui.add_space(6.0);
                                    ui.checkbox(&mut self.active_host_profile.public_lan_announce, "Анонсировать в локальной сети (LAN Broadcast)");
                                }
                                HostConfigTab::Sandbox => {
                                    ui.checkbox(&mut self.active_host_profile.sandbox.pvp_enabled, "Включить PvP (урон между игроками)");
                                    ui.add_space(4.0);
                                    ui.checkbox(&mut self.active_host_profile.sandbox.structural_integrity, "Сопромат построек (физическое обрушение)");
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Множитель популяции:");
                                        ui.add(egui::Slider::new(&mut self.active_host_profile.sandbox.population_multiplier, 0.1..=5.0).text("x"));
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Гравитация:");
                                        ui.add(egui::Slider::new(&mut self.active_host_profile.sandbox.gravity_multiplier, 0.1..=3.0).text("x"));
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Режим спавна:");
                                        let modes = ["Safe", "Migration", "Hardcore", "NoRespawnCleaned"];
                                        for m in modes {
                                            if ui.selectable_label(self.active_host_profile.sandbox.spawn_mode == m, m).clicked() {
                                                self.active_host_profile.sandbox.spawn_mode = m.to_string();
                                            }
                                        }
                                    });
                                }
                                HostConfigTab::World => {
                                    ui.horizontal(|ui| {
                                        ui.label("Папка сохранения:");
                                        ui.add_sized([260.0, 22.0], egui::TextEdit::singleline(&mut self.active_host_profile.save_folder_name));
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Сид мира:");
                                        let mut seed_str = self.active_host_profile.world_seed.to_string();
                                        if ui.add_sized([160.0, 22.0], egui::TextEdit::singleline(&mut seed_str)).changed() {
                                            if let Ok(s) = seed_str.parse() {
                                                self.active_host_profile.world_seed = s;
                                            }
                                        }
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Режим игры:");
                                        let is_surv = self.active_host_profile.experience_id == "survival";
                                        if ui.selectable_label(is_surv, "Выживание").clicked() {
                                            self.active_host_profile.experience_id = "survival".to_string();
                                        }
                                        let is_creat = self.active_host_profile.experience_id == "creative";
                                        if ui.selectable_label(is_creat, "Креатив").clicked() {
                                            self.active_host_profile.experience_id = "creative".to_string();
                                        }
                                    });
                                }
                            }

                            if let Some((msg, is_err)) = &self.host_status_msg {
                                ui.add_space(6.0);
                                let col = if *is_err { egui::Color32::RED } else { egui::Color32::GREEN };
                                ui.colored_label(col, msg);
                            }

                            ui.add_space(14.0);
                            ui.separator();
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui.add_sized([220.0, 36.0], egui::Button::new(
                                    egui::RichText::new("Запустить сервер").strong().color(egui::Color32::WHITE)
                                )).clicked() {
                                    start_host_session_requested = true;
                                }
                                if ui.add_sized([140.0, 36.0], egui::Button::new("Назад")).clicked() {
                                    back_to_title_requested = true;
                                }
                            });
                        });
                }
                MenuScreen::JoinServer => {
                    egui::Window::new("JoinServerScreen")
                        .collapsible(false)
                        .resizable(false)
                        .title_bar(false)
                        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                        .frame(
                            egui::Frame::NONE
                                .fill(egui::Color32::from_black_alpha(195))
                                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(45)))
                                .corner_radius(10.0)
                                .inner_margin(egui::Margin::symmetric(28, 22)),
                        )
                        .fixed_size(egui::vec2(460.0, 420.0))
                        .show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("СЕТЕВАЯ ИГРА (JOIN SERVER)")
                                        .size(19.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.label(
                                    egui::RichText::new("Поиск локальных LAN серверов или прямое подключение")
                                        .size(11.0)
                                        .color(egui::Color32::from_white_alpha(140)),
                                );
                                ui.add_space(10.0);
                            });

                            ui.horizontal(|ui| {
                                ui.label("Ваш никнейм:");
                                if ui.add_sized([180.0, 22.0], egui::TextEdit::singleline(&mut self.player_name_input)).changed() {
                                    self.settings.player_name = self.player_name_input.clone();
                                    settings_changed = true;
                                }
                            });
                            ui.add_space(6.0);
                            ui.separator();

                            ui.horizontal(|ui| {
                                if ui.selectable_label(self.join_tab == JoinTab::Lan, "LAN-серверы").clicked() {
                                    self.join_tab = JoinTab::Lan;
                                }
                                if ui.selectable_label(self.join_tab == JoinTab::Direct, "Прямое подключение").clicked() {
                                    self.join_tab = JoinTab::Direct;
                                }
                            });
                            ui.separator();
                            ui.add_space(6.0);

                            match self.join_tab {
                                JoinTab::Lan => {
                                    if self.discovery_scanner.servers.is_empty() {
                                        ui.vertical_centered(|ui| {
                                            ui.add_space(20.0);
                                            ui.label(egui::RichText::new("Поиск серверов в локальной сети...").color(egui::Color32::GRAY));
                                            ui.weak("(Убедитесь, что хост включил 'Анонсировать в LAN')");
                                        });
                                    } else {
                                        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                                            for server in self.discovery_scanner.servers.clone() {
                                                ui.group(|ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.vertical(|ui| {
                                                            ui.label(egui::RichText::new(&server.name).strong().color(egui::Color32::WHITE));
                                                            ui.weak(format!("IP: {} | Игроков: {}", server.addr, server.player_count));
                                                        });
                                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                            if ui.button("Войти").clicked() {
                                                                let pw = if self.join_password_input.is_empty() { None } else { Some(self.join_password_input.clone()) };
                                                                connect_to_server_requested = Some((server.addr, pw));
                                                            }
                                                        });
                                                    });
                                                });
                                            }
                                        });
                                    }
                                }
                                JoinTab::Direct => {
                                    ui.horizontal(|ui| {
                                        ui.label("IP / Хост:");
                                        ui.add_sized([220.0, 22.0], egui::TextEdit::singleline(&mut self.connect_ip_input).hint_text("127.0.0.1"));
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Порт:");
                                        ui.add_sized([90.0, 22.0], egui::TextEdit::singleline(&mut self.server_port_input).hint_text("25565"));
                                    });
                                }
                            }

                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.label("Пароль сервера:");
                                ui.add_sized([160.0, 22.0], egui::TextEdit::singleline(&mut self.join_password_input).password(true).hint_text("если требуется"));
                            });

                            if let Some((msg, color, ts)) = &self.net_status {
                                if ts.elapsed().as_secs() < 8 {
                                    ui.add_space(6.0);
                                    ui.colored_label(*color, msg);
                                }
                            }

                            ui.add_space(12.0);
                            ui.separator();
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if self.join_tab == JoinTab::Direct {
                                    if ui.add_sized([180.0, 36.0], egui::Button::new(
                                        egui::RichText::new("Подключиться").strong().color(egui::Color32::WHITE)
                                    )).clicked() {
                                        connect_requested = true;
                                    }
                                }
                                if ui.add_sized([120.0, 36.0], egui::Button::new("Назад")).clicked() {
                                    back_to_title_requested = true;
                                }
                            });
                        });
                }
            }

            // 4. Inventory Window [E] (Creative Palette or Survival Backpack)
            if inventory_open && self.screen == MenuScreen::Playing {
                if player_mode == PlayerMode::Creative {
                    egui::Window::new("Креативный инвентарь")
                        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                        .collapsible(false)
                        .resizable(false)
                        .title_bar(false)
                        .frame(
                            egui::Frame::window(&ctx.style())
                                .fill(egui::Color32::from_rgb(22, 26, 34))
                                .stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(70, 90, 120)))
                                .corner_radius(10.0)
                                .inner_margin(egui::Margin::symmetric(16, 14)),
                        )
                        .fixed_size(egui::vec2(440.0, 480.0))
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Выбор блоков (Креатив)")
                                        .size(15.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("Очистить инвентарь").clicked() {
                                        clear_inventory_requested = true;
                                    }
                                });
                            });
                            ui.add_space(6.0);

                            ui.horizontal(|ui| {
                                ui.label("Поиск:");
                                ui.add_sized(
                                    [240.0, 22.0],
                                    egui::TextEdit::singleline(&mut self.creative_search)
                                        .hint_text("Название блока..."),
                                );
                                if !self.creative_search.is_empty() && ui.button("X").clicked() {
                                    self.creative_search.clear();
                                }
                            });
                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(6.0);

                            // Block Palette Grid
                            egui::ScrollArea::vertical()
                                .max_height(250.0)
                                .show(ui, |ui| {
                                    let query = self.creative_search.trim().to_lowercase();
                                    let blocks: Vec<(BlockId, String, egui::Color32)> = registry
                                        .solid_blocks()
                                        .into_iter()
                                        .filter_map(|(id, def)| {
                                            let name = def.name.split(':').last().unwrap_or(&def.name).to_string();
                                            if query.is_empty() || name.to_lowercase().contains(&query) || def.name.to_lowercase().contains(&query) {
                                                let color = egui::Color32::from_rgb(
                                                    def.icon_color[0],
                                                    def.icon_color[1],
                                                    def.icon_color[2],
                                                );
                                                Some((id, name, color))
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();

                                    egui::Grid::new("creative_palette_grid")
                                        .spacing([6.0, 6.0])
                                        .show(ui, |ui| {
                                            for (idx, (id, name, _color)) in blocks.iter().enumerate() {
                                                let (rect, resp) = ui.allocate_exact_size(
                                                    egui::vec2(44.0, 44.0),
                                                    egui::Sense::click(),
                                                );
                                                let is_hov = resp.hovered();
                                                ui.painter().rect_filled(
                                                    rect,
                                                    6.0,
                                                    if is_hov {
                                                        egui::Color32::from_rgb(46, 54, 68)
                                                    } else {
                                                        egui::Color32::from_rgb(24, 28, 36)
                                                    },
                                                );
                                                ui.painter().rect_stroke(
                                                    rect,
                                                    6.0,
                                                    egui::Stroke::new(
                                                        1.0_f32,
                                                        if is_hov {
                                                            egui::Color32::WHITE
                                                        } else {
                                                            egui::Color32::from_white_alpha(45)
                                                        },
                                                    ),
                                                    egui::StrokeKind::Inside,
                                                );

                                                render_block_icon(
                                                    ui.painter(),
                                                    rect.center(),
                                                    22.0,
                                                    *id,
                                                    &registry,
                                                );

                                                let resp = resp.on_hover_text(name);
                                                if resp.clicked() {
                                                    creative_pick_block = Some(*id);
                                                }

                                                if (idx + 1) % 9 == 0 {
                                                    ui.end_row();
                                                }
                                            }
                                        });
                                });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(6.0);

                            // Hotbar (1..9)
                            ui.label(
                                egui::RichText::new("Панель быстрого доступа (1-9):")
                                    .size(12.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                            ui.horizontal(|ui| {
                                for slot_idx in 0..9 {
                                    let slot = inventory_slots[slot_idx];
                                    let is_active = slot_idx == selected_slot;
                                    render_inventory_slot(
                                        ui,
                                        slot_idx,
                                        slot,
                                        is_active,
                                        &registry,
                                        &mut slot_action,
                                    );
                                }
                            });

                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if ui.button(egui::RichText::new("Закрыть [E]").size(13.0)).clicked() {
                                    close_inventory_requested = true;
                                }
                                if let Some(held) = &held_stack {
                                    let name = registry
                                        .get(held.block_id)
                                        .map(|d| d.name.split(':').last().unwrap_or(&d.name))
                                        .unwrap_or("блок");
                                    ui.label(
                                        egui::RichText::new(format!("В руке: {} x{}", name, held.count))
                                            .color(egui::Color32::YELLOW)
                                            .size(13.0),
                                    );
                                }
                            });
                        });
                } else {
                    egui::Window::new("Рюкзак и Инвентарь [E]")
                        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                        .collapsible(false)
                        .resizable(false)
                        .frame(
                            egui::Frame::window(&ctx.style())
                                .fill(egui::Color32::from_rgb(22, 26, 34))
                                .stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(70, 90, 120)))
                                .corner_radius(10.0)
                                .inner_margin(egui::Margin::symmetric(16, 14)),
                        )
                        .show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("Рюкзак и инвентарь (36 слотов)")
                                        .size(15.0)
                                        .color(egui::Color32::WHITE),
                                );
                            });
                            ui.add_space(8.0);

                            // 1. Backpack: 3 rows of 9 slots (9..36)
                            ui.label(egui::RichText::new("Рюкзак:").size(12.0).color(egui::Color32::LIGHT_GRAY));
                            for row in 0..3 {
                                ui.horizontal(|ui| {
                                    for col in 0..9 {
                                        let slot_idx = 9 + row * 9 + col;
                                        let slot = inventory_slots[slot_idx];
                                        render_inventory_slot(
                                            ui,
                                            slot_idx,
                                            slot,
                                            false,
                                            &registry,
                                            &mut slot_action,
                                        );
                                    }
                                });
                                ui.add_space(3.0);
                            }

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(6.0);

                            // 2. Hotbar: 1 row of 9 slots (0..9)
                            ui.label(egui::RichText::new("Панель быстрого доступа (1-9):").size(12.0).color(egui::Color32::LIGHT_GRAY));
                            ui.horizontal(|ui| {
                                for slot_idx in 0..9 {
                                    let slot = inventory_slots[slot_idx];
                                    let is_active = slot_idx == selected_slot;
                                    render_inventory_slot(
                                        ui,
                                        slot_idx,
                                        slot,
                                        is_active,
                                        &registry,
                                        &mut slot_action,
                                    );
                                }
                            });

                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button(egui::RichText::new("Закрыть [E]").size(13.0)).clicked() {
                                    close_inventory_requested = true;
                                }
                                if let Some(held) = &held_stack {
                                    let name = registry
                                        .get(held.block_id)
                                        .map(|d| d.name.split(':').last().unwrap_or(&d.name))
                                        .unwrap_or("блок");
                                    ui.label(
                                        egui::RichText::new(format!("В руке: {} x{}", name, held.count))
                                            .color(egui::Color32::YELLOW)
                                            .size(13.0),
                                    );
                                }
                            });
                        });
                }
            }

            // Floating held item cursor in inventory
            if inventory_open {
                if let Some(held) = held_stack {
                    if let Some(mouse_pos) = ctx.pointer_latest_pos() {
                        let painter = ctx.layer_painter(egui::LayerId::new(
                            egui::Order::Tooltip,
                            egui::Id::new("floating_held_item"),
                        ));
                        render_block_icon(&painter, mouse_pos, 28.0, held.block_id, &registry);
                        let count_text = format!("{}", held.count);
                        let text_pos = mouse_pos + egui::vec2(12.0, 10.0);
                        painter.text(
                            text_pos + egui::vec2(1.0, 1.0),
                            egui::Align2::LEFT_BOTTOM,
                            &count_text,
                            egui::FontId::proportional(13.0),
                            egui::Color32::BLACK,
                        );
                        painter.text(
                            text_pos,
                            egui::Align2::LEFT_BOTTOM,
                            &count_text,
                            egui::FontId::proportional(13.0),
                            egui::Color32::WHITE,
                        );
                    }
                }
            }
        });

        if let Some(action) = slot_action {
            match action {
                SlotAction::LeftClick(idx) => {
                    match (self.held_stack.take(), self.inventory.slots[idx]) {
                        (None, Some(slot_item)) => {
                            self.held_stack = Some(slot_item);
                            self.inventory.slots[idx] = None;
                        }
                        (Some(held), None) => {
                            self.inventory.slots[idx] = Some(held);
                        }
                        (Some(held), Some(slot_item)) => {
                            if held.block_id == slot_item.block_id {
                                let space = ItemStack::MAX_STACK.saturating_sub(slot_item.count);
                                let to_add = held.count.min(space);
                                self.inventory.slots[idx] = Some(ItemStack {
                                    block_id: slot_item.block_id,
                                    count: slot_item.count + to_add,
                                });
                                let rem = held.count - to_add;
                                if rem > 0 {
                                    self.held_stack = Some(ItemStack {
                                        block_id: held.block_id,
                                        count: rem,
                                    });
                                }
                            } else {
                                self.held_stack = Some(slot_item);
                                self.inventory.slots[idx] = Some(held);
                            }
                        }
                        (None, None) => {}
                    }
                }
                SlotAction::RightClick(idx) => {
                    match (self.held_stack.take(), self.inventory.slots[idx]) {
                        (None, Some(slot_item)) => {
                            // Split stack: take half in hand, leave rest in slot
                            let take_count = (slot_item.count + 1) / 2;
                            let rem = slot_item.count - take_count;
                            self.held_stack = Some(ItemStack {
                                block_id: slot_item.block_id,
                                count: take_count,
                            });
                            if rem > 0 {
                                self.inventory.slots[idx] = Some(ItemStack {
                                    block_id: slot_item.block_id,
                                    count: rem,
                                });
                            } else {
                                self.inventory.slots[idx] = None;
                            }
                        }
                        (Some(mut held), None) => {
                            // Place 1 item from hand into empty slot
                            self.inventory.slots[idx] = Some(ItemStack {
                                block_id: held.block_id,
                                count: 1,
                            });
                            if held.count > 1 {
                                held.count -= 1;
                                self.held_stack = Some(held);
                            }
                        }
                        (Some(mut held), Some(mut slot_item)) => {
                            if held.block_id == slot_item.block_id {
                                if slot_item.count < ItemStack::MAX_STACK {
                                    slot_item.count += 1;
                                    self.inventory.slots[idx] = Some(slot_item);
                                    if held.count > 1 {
                                        held.count -= 1;
                                        self.held_stack = Some(held);
                                    }
                                } else {
                                    self.held_stack = Some(held);
                                }
                            } else {
                                // Swap different items
                                self.held_stack = Some(slot_item);
                                self.inventory.slots[idx] = Some(held);
                            }
                        }
                        (None, None) => {}
                    }
                }
                SlotAction::ShiftClick(idx) => {
                    // Quick move: hotbar (0..9) <-> backpack (9..36)
                    if let Some(mut item) = self.inventory.slots[idx] {
                        let target_range = if idx < 9 { 9..36 } else { 0..9 };
                        // 1. Try to merge into existing stacks
                        for t_idx in target_range.clone() {
                            if let Some(t_item) = &mut self.inventory.slots[t_idx] {
                                if t_item.block_id == item.block_id && t_item.count < ItemStack::MAX_STACK {
                                    let space = ItemStack::MAX_STACK - t_item.count;
                                    let to_add = item.count.min(space);
                                    t_item.count += to_add;
                                    item.count -= to_add;
                                    if item.count == 0 {
                                        break;
                                    }
                                }
                            }
                        }
                        // 2. Put remainder into first empty slot in target range
                        if item.count > 0 {
                            for t_idx in target_range {
                                if self.inventory.slots[t_idx].is_none() {
                                    self.inventory.slots[t_idx] = Some(item);
                                    item.count = 0;
                                    break;
                                }
                            }
                        }
                        if item.count == 0 {
                            self.inventory.slots[idx] = None;
                        } else {
                            self.inventory.slots[idx] = Some(item);
                        }
                    }
                }
            }
        }

        if let Some(id) = creative_pick_block {
            self.held_stack = Some(ItemStack::new(id, 64));
        }
        if clear_inventory_requested {
            self.inventory.slots.fill(None);
            self.held_stack = None;
        }
        if respawn_requested {
            self.health = 100.0;
            self.last_attacker_name = None;
            self.last_damage_info = None;
            self.death_fall_progress = 0.0;
            let respawn_pos = self.mode.spawn_position();
            self.camera.position = respawn_pos;
            self.player_pos = respawn_pos;
            self.velocity = Vec3::ZERO;
            self.target_pitch = 0.0;
            self.camera.pitch = 0.0;
            self.set_cursor_lock(true);
            audio::play_sound(audio::SoundEffect::Respawn);
            let my_id = if self.server_host.is_some() {
                0
            } else if let Some(client) = &self.game_client {
                client.my_id
            } else {
                0
            };
            let hp_pkt = Packet::PlayerHealth {
                player_id: my_id,
                health: 100.0,
            };
            if let Some(host) = &mut self.server_host {
                host.broadcast(&hp_pkt);
            } else if let Some(client) = &mut self.game_client {
                let _ = client.send_packet(&hp_pkt);
            }
        }

        if let Some((name, pos)) = tp_to_player_requested {
            self.camera.position = pos + Vec3::new(0.0, 0.2, 0.0);
            self.velocity = Vec3::ZERO;
            audio::play_sound(audio::SoundEffect::Teleport);
            self.push_chat(
                format!("Телепортирован к игроку {name}"),
                egui::Color32::from_rgb(100, 200, 255),
            );
            self.screen = MenuScreen::Playing;
            self.set_cursor_lock(true);
        }

        if close_inventory_requested {
            self.inventory_open = false;
            self.set_cursor_lock(true);
            if let Some(held) = self.held_stack.take() {
                self.inventory.add_item(held.block_id, held.count);
            }
        }


        if submit_chat_requested {
            self.submit_chat();
        }
        if reset_settings_requested {
            self.settings = Settings::default();
            self.rebinding = None;
            settings_changed = true;
        }
        if settings_changed {
            self.apply_settings();
        }
        if start_server_requested {
            let port = self
                .server_port_input
                .trim()
                .parse::<u16>()
                .unwrap_or(DEFAULT_PORT);
            let initial_edits = self.world.get_all_raw_edits();
            let mut prof = self.active_host_profile.clone();
            prof.port = port;
            match ServerHost::start(prof, self.time_of_day, initial_edits) {
                Ok(host) => {
                    let my_name = self.settings.player_name.clone();
                    self.discovery_beacon = Some(DiscoveryBeacon::new(my_name, port));
                    self.server_host = Some(host);
                    self.camera.position = self.player_pos;
                    self.camera.yaw = self.player_yaw;
                    self.camera.pitch = self.player_pitch;
                    self.target_yaw = self.player_yaw;
                    self.target_pitch = self.player_pitch;
                    self.resume_game();
                    self.push_chat(
                        format!("[Сеть] Сервер запущен на порту {port}! Друзья могут подключаться."),
                        egui::Color32::from_rgb(120, 220, 120),
                    );
                }
                Err(e) => {
                    self.net_status = Some((
                        format!("Ошибка запуска сервера на порту {port}: {e}"),
                        egui::Color32::RED,
                        Instant::now(),
                    ));
                }
            }
        }


        if stop_server_requested {
            self.server_host = None;
            self.discovery_beacon = None;
            self.host_remote_players.clear();
            self.net_status = Some(("Сервер остановлен".to_string(), egui::Color32::YELLOW, Instant::now()));
            self.push_chat("[Сеть] Сервер остановлен", egui::Color32::YELLOW);
        }

        if connect_requested {
            let ip_str = self.connect_ip_input.trim();
            let addr_str = if ip_str.contains(':') {
                ip_str.to_string()
            } else {
                format!("{ip_str}:{DEFAULT_PORT}")
            };
            match addr_str.parse::<SocketAddr>() {
                Ok(addr) => {
                    self.settings.last_connected_ip = ip_str.to_string();
                    self.apply_settings();
                    let pw = if self.join_password_input.trim().is_empty() { None } else { Some(self.join_password_input.trim().to_string()) };
                    match GameClient::connect(addr, &self.settings.player_name, pw) {
                        Ok(client) => {
                            self.game_client = Some(client);
                            self.screen = MenuScreen::Playing;
                            self.set_cursor_lock(true);
                            self.push_chat(
                                format!("[Сеть] Подключение к {addr}..."),
                                egui::Color32::from_rgb(180, 210, 255),
                            );
                        }
                        Err(e) => {
                            self.net_status = Some((
                                format!("Не удалось подключиться к {addr}: {e}"),
                                egui::Color32::RED,
                                Instant::now(),
                            ));
                        }
                    }
                }
                Err(_) => {
                    self.net_status = Some((
                        "Неверный формат IP адреса (нужен IP или IP:порт)".to_string(),
                        egui::Color32::RED,
                        Instant::now(),
                    ));
                }
            }
        }


        if disconnect_client_requested {
            self.game_client = None;
            self.net_status = Some(("Отключено от сервера".to_string(), egui::Color32::YELLOW, Instant::now()));
            self.push_chat("[Сеть] Отключено от сервера", egui::Color32::YELLOW);
        }

        if open_select_world_requested {
            self.open_select_world();
        } else if open_create_world_requested {
            self.screen = MenuScreen::CreateWorld;
        } else if cancel_create_world_requested {
            self.open_select_world();
        } else if let Some(id) = select_world_id_requested {
            self.selected_world_id = Some(id);
        } else if play_selected_world_requested {
            if let Some(id) = &self.selected_world_id {
                if let Some(meta) = self.cached_world_list.iter().find(|w| &w.id == id).cloned() {
                    self.session_type = SessionType::Solo;
                    self.server_host = None;
                    self.discovery_beacon = None;
                    self.game_client = None;
                    self.load_world(meta);
                }
            }
        } else if delete_selected_world_requested {
            if let Some(id) = &self.selected_world_id {
                let _ = WorldManager::delete_world(id);
                self.cached_world_list = WorldManager::list_worlds();
                self.selected_world_id = self.cached_world_list.first().map(|w| w.id.clone());
            }
        } else if back_to_title_from_select_requested {
            self.screen = MenuScreen::Title;
        } else if create_and_start_world_requested {
            let seed_num = self.create_world_seed.trim().parse::<u32>().unwrap_or_else(|_| {
                let mut h: u32 = 0;
                for b in self.create_world_seed.bytes() {
                    h = h.wrapping_mul(31).wrapping_add(b as u32);
                }
                h
            });
            let name = if self.create_world_name.trim().is_empty() {
                "Новый мир"
            } else {
                self.create_world_name.trim()
            };
            let spawn_pos = if self.create_world_gen == "Lithosphere" {
                let gen = LithosphereWorldGen::new(seed_num, &self.registry);
                let (sy, _) = gen.surface_height(0, 0);
                [0.5, (sy + 2).max(10) as f32, 0.5]
            } else {
                [0.5, 20.0, 0.5]
            };

            match WorldManager::create_world(
                name,
                seed_num,
                &self.create_world_mode,
                &self.create_world_gen,
                spawn_pos,
            ) {
                Ok(meta) => {
                    self.cached_world_list = WorldManager::list_worlds();
                    self.selected_world_id = Some(meta.id.clone());
                    self.session_type = SessionType::Solo;
                    self.server_host = None;
                    self.discovery_beacon = None;
                    self.game_client = None;
                    self.load_world(meta);
                }
                Err(e) => {
                    self.push_chat(&format!("[Ошибка создания мира] {e}"), egui::Color32::RED);
                }
            }
        } else if back_to_title_requested {
            self.save_current_world();
            self.player_pos = self.camera.position;
            self.player_yaw = self.camera.yaw;
            self.player_pitch = self.camera.pitch;
            self.set_cursor_lock(false);
            self.screen = MenuScreen::Title;
        } else if open_host_server_requested {
            self.return_screen = MenuScreen::Title;
            self.screen = MenuScreen::HostServer;
        } else if open_join_server_requested {
            self.return_screen = MenuScreen::Title;
            self.screen = MenuScreen::JoinServer;
        } else if new_host_profile_requested {
            let new_name = format!("Profile_{}", self.server_profiles.len() + 1);
            let mut new_prof = ServerHostProfile::default();
            new_prof.server_name = format!("Server {}", self.server_profiles.len() + 1);
            new_prof.save_folder_name = format!("world_host_{}", self.server_profiles.len() + 1);
            let _ = WorldManager::save_server_profile(&new_name, &new_prof);
            self.server_profiles = WorldManager::list_server_profiles();
            self.host_profile_name_input = new_name;
            self.active_host_profile = new_prof;
            self.host_status_msg = Some(("Создан новый профиль сервера".to_string(), false));
        } else if save_host_profile_requested {
            match WorldManager::save_server_profile(&self.host_profile_name_input, &self.active_host_profile) {
                Ok(()) => {
                    self.server_profiles = WorldManager::list_server_profiles();
                    self.host_status_msg = Some(("Настройки профиля сохранены".to_string(), false));
                }
                Err(e) => {
                    self.host_status_msg = Some((format!("Ошибка сохранения: {e}"), true));
                }
            }
        } else if delete_host_profile_requested {
            let _ = WorldManager::delete_server_profile(&self.host_profile_name_input);
            self.server_profiles = WorldManager::list_server_profiles();
            if let Some((name, prof)) = self.server_profiles.first() {
                self.host_profile_name_input = name.clone();
                self.active_host_profile = prof.clone();
            }
            self.host_status_msg = Some(("Профиль удалён".to_string(), false));
        } else if start_host_session_requested {
            // Save current profile to disk
            let _ = WorldManager::save_server_profile(&self.host_profile_name_input, &self.active_host_profile);
            self.server_profiles = WorldManager::list_server_profiles();

            // Ensure world exists for this save_folder_name or create new
            let folder_name = &self.active_host_profile.save_folder_name;
            let mut world_meta = WorldManager::list_worlds().into_iter().find(|w| w.id == *folder_name);
            if world_meta.is_none() {
                let seed_u32 = self.active_host_profile.world_seed as u32;
                let mode_str = if self.active_host_profile.experience_id == "creative" { "Creative" } else { "Survival" };
                let created = WorldManager::create_world(
                    &self.active_host_profile.server_name,
                    seed_u32,
                    mode_str,
                    "Lithosphere",
                    [0.5, 42.0, 0.5],
                );
                if let Ok(meta) = created {
                    world_meta = Some(meta);
                }
            }

            if let Some(meta) = world_meta {
                self.load_world(meta);
            }

            let initial_edits = self.world.get_all_raw_edits();
            match ServerHost::start(self.active_host_profile.clone(), self.time_of_day, initial_edits) {
                Ok(host) => {
                    let port = host.port();
                    let s_name = host.profile.server_name.clone();
                    let announce = host.profile.public_lan_announce;
                    self.server_host = Some(host);
                    if announce {
                        self.discovery_beacon = Some(DiscoveryBeacon::new(s_name, port));
                    } else {
                        self.discovery_beacon = None;
                    }
                    self.game_client = None;
                    self.session_type = SessionType::Host;
                    self.screen = MenuScreen::Playing;
                    self.set_cursor_lock(true);
                    self.push_chat(
                        format!("[Хост] Сервер '{}' запущен на порту {port}!", self.active_host_profile.server_name),
                        egui::Color32::from_rgb(100, 255, 120),
                    );
                }
                Err(e) => {
                    self.host_status_msg = Some((format!("Ошибка запуска сервера: {e}"), true));
                }
            }
        } else if let Some((addr, pw)) = connect_to_server_requested {
            let p_name = self.settings.player_name.clone();
            match GameClient::connect(addr, &p_name, pw) {
                Ok(client) => {
                    self.game_client = Some(client);
                    self.server_host = None;
                    self.discovery_beacon = None;
                    self.session_type = SessionType::Client;
                    self.screen = MenuScreen::Playing;
                    self.set_cursor_lock(true);
                    self.push_chat(
                        format!("[Сеть] Подключение к серверу {addr}..."),
                        egui::Color32::from_rgb(100, 220, 255),
                    );
                }
                Err(e) => {
                    self.net_status = Some((format!("Ошибка подключения: {e}"), egui::Color32::RED, Instant::now()));
                }
            }
        } else if connect_requested {
            let port = self.server_port_input.trim().parse::<u16>().unwrap_or(25565);
            let host_ip = if self.connect_ip_input.trim().is_empty() {
                "127.0.0.1"
            } else {
                self.connect_ip_input.trim()
            };
            let addr_str = if host_ip.contains(':') {
                host_ip.to_string()
            } else {
                format!("{host_ip}:{port}")
            };
            if let Ok(addr) = addr_str.parse::<SocketAddr>() {
                let pw = if self.join_password_input.trim().is_empty() { None } else { Some(self.join_password_input.trim().to_string()) };
                let p_name = self.settings.player_name.clone();
                match GameClient::connect(addr, &p_name, pw) {
                    Ok(client) => {
                        self.game_client = Some(client);
                        self.server_host = None;
                        self.discovery_beacon = None;
                        self.session_type = SessionType::Client;
                        self.screen = MenuScreen::Playing;
                        self.set_cursor_lock(true);
                        self.push_chat(
                            format!("[Сеть] Подключение к серверу {addr}..."),
                            egui::Color32::from_rgb(100, 220, 255),
                        );
                    }
                    Err(e) => {
                        self.net_status = Some((format!("Ошибка подключения: {e}"), egui::Color32::RED, Instant::now()));
                    }
                }
            } else {
                self.net_status = Some(("Неверный IP-адрес сервера".to_string(), egui::Color32::RED, Instant::now()));
            }
        } else if let Some(id) = kick_client_requested {
            if let Some(host) = &mut self.server_host {
                host.kick_client(id, "Кикнут хостом сервера");
                self.push_chat(format!("[Хост] Игрок #{id} кикнут с сервера"), egui::Color32::YELLOW);
            }
        } else if let Some(id) = teleport_client_to_host_requested {
            if let Some(host) = &mut self.server_host {
                host.teleport_client(id, self.player_pos);
                self.push_chat(format!("[Хост] Игрок #{id} телепортирован к хосту"), egui::Color32::GREEN);
            }
        } else if apply_live_rules_requested {
            if let Some(host) = &mut self.server_host {
                host.update_sandbox_rules(self.active_host_profile.sandbox.clone());
                self.push_chat("[Хост] Обновленные правила песочницы применены для всех игроков!", egui::Color32::from_rgb(100, 255, 120));
            }
        } else if resume_requested {
            self.resume_game();
        } else if open_settings_requested {
            self.return_screen = if self.screen == MenuScreen::Title {
                MenuScreen::Title
            } else {
                MenuScreen::Pause
            };
            self.screen = MenuScreen::Settings;
        } else if open_multiplayer_requested {
            self.return_screen = MenuScreen::Title;
            self.screen = MenuScreen::JoinServer;
        } else if back_to_pause_requested {
            self.rebinding = None;
            self.screen = self.return_screen;
        } else if quit_requested {
            self.save_current_world();
            event_loop.exit();
        }

        full_output
    }
}

fn register_built_in_commands(cmd: &mut CommandRegistry) {
    cmd.register(
        "gamemode",
        &["gm"],
        "/gamemode <survival|creative|0|1>",
        "Переключить режим игры",
        |args, host| {
            if args.is_empty() {
                return Err("Использование: /gamemode <survival|creative|0|1>".to_string());
            }
            match args[0].to_ascii_lowercase().as_str() {
                "survival" | "0" | "s" => {
                    host.set_player_mode(PlayerMode::Survival);
                    Ok("Режим игры переключён на Выживание".to_string())
                }
                "creative" | "1" | "c" => {
                    host.set_player_mode(PlayerMode::Creative);
                    Ok("Режим игры переключён на Креатив (двойной пробел для полёта)".to_string())
                }
                other => Err(format!("Неизвестный режим '{other}'. Доступно: survival, creative")),
            }
        }
,
    );

    cmd.register(
        "fly",
        &[],
        "/fly [on|off]",
        "Включить или выключить полёт",
        |args, host| {
            let cur = host.is_flying();
            let new_state = if args.is_empty() {
                !cur
            } else {
                match args[0].to_ascii_lowercase().as_str() {
                    "on" | "true" | "1" => true,
                    "off" | "false" | "0" => false,
                    _ => !cur,
                }
            };
            if new_state && !host.can_fly() {
                host.set_can_fly(true);
            }
            host.set_flying(new_state);
            let state_str = if new_state { "включён" } else { "выключен" };
            Ok(format!("Полёт {state_str}"))
        },
    );

    cmd.register(
        "tp",
        &["teleport"],
        "/tp <игрок> ИЛИ /tp <x> <y> <z>",
        "Телепортироваться к игроку или по координатам",
        |args, host| {
            if args.is_empty() {
                let cur = host.player_position();
                return Err(format!(
                    "Использование: /tp <игрок> или /tp <x> <y> <z>. Текущие координаты: {:.1} {:.1} {:.1}",
                    cur.x, cur.y, cur.z
                ));
            }

            if args.len() == 1 {
                let target_name = args[0];
                if let Some((name, pos)) = host.find_player(target_name) {
                    host.teleport_player(pos + Vec3::new(0.0, 0.2, 0.0));
                    audio::play_sound(audio::SoundEffect::Teleport);
                    return Ok(format!("Телепортирован к игроку {name}"));
                } else {
                    return Err(format!("Игрок '{target_name}' не найден в игре"));
                }
            }

            if args.len() < 3 {
                return Err("Использование: /tp <игрок> или /tp <x> <y> <z>".to_string());
            }

            let cur = host.player_position();
            let parse_coord = |s: &str, cur: f32| -> Result<f32, String> {
                if let Some(rest) = s.strip_prefix('~') {
                    if rest.is_empty() {
                        Ok(cur)
                    } else {
                        rest.parse::<f32>()
                            .map(|v| cur + v)
                            .map_err(|_| format!("Неверное число '{s}'"))
                    }
                } else {
                    s.parse::<f32>()
                        .map_err(|_| format!("Неверное число '{s}'"))
                }
            };

            let x = parse_coord(args[0], cur.x)?;
            let y = parse_coord(args[1], cur.y)?;
            let z = parse_coord(args[2], cur.z)?;

            host.teleport_player(Vec3::new(x, y, z));
            audio::play_sound(audio::SoundEffect::Teleport);
            Ok(format!("Телепортирован на {:.1}, {:.1}, {:.1}", x, y, z))
        },
    );

    cmd.register(
        "heal",
        &["health", "hp"],
        "/heal [количество]",
        "Восстановить здоровье игрока",
        |args, host| {
            let amount = if args.is_empty() {
                100.0
            } else {
                args[0].parse::<f32>().unwrap_or(100.0)
            };
            host.heal_player(amount);
            Ok(format!("Здоровье восстановлено (+{:.0} HP)", amount))
        },
    );

    cmd.register(
        "block",
        &["give", "item"],
        "/block [имя_блока]",
        "Выбрать активный блок для установки или показать список",
        |args, host| {
            let reg = host.block_registry();
            if args.is_empty() {
                let list: Vec<String> = reg
                    .solid_blocks()
                    .into_iter()
                    .map(|(_, def)| def.name.clone())
                    .collect();
                return Ok(format!("Доступные блоки: {}", list.join(", ")));
            }
            let query = args[0];
            let (id, name) = {
                let reg = host.block_registry();
                let Some(id) = reg.find_fuzzy(query) else {
                    return Err(format!("Блок '{query}' не найден. Введите /block для списка"));
                };
                let name = reg.get(id).map(|d| d.name.clone()).unwrap_or_else(|| "unknown".to_string());
                (id, name)
            };
            host.set_selected_block(id);
            Ok(format!("Активный блок выбран: {name}"))
        },
    );

    cmd.register(
        "clear",
        &["cls"],
        "/clear",
        "Очистить историю сообщений чата",
        |_args, host| {
            host.clear_chat();
            Ok(String::new())
        },
    );

    cmd.register(
        "save",
        &[],
        "/save",
        "Сохранить изменения мира в world.dat",
        |_args, host| {
            let count = host.save_world()?;
            Ok(format!("Мир сохранён в world.dat (изменённых блоков: {count})"))
        },
    );

    cmd.register(
        "time",
        &[],
        "/time <set <0..24|day|noon|sunset|night>|add <часы>|query>",
        "Управление временем суток",
        |args, host| {
            if args.is_empty() {
                let cur = host.time_of_day();
                let hours = cur.floor() as u32;
                let minutes = ((cur - hours as f32) * 60.0).floor() as u32;
                return Ok(format!("Текущее время: {:02}:{:02} ({:.2} ч)", hours, minutes, cur));
            }
            match args[0].to_ascii_lowercase().as_str() {
                "set" => {
                    if args.len() < 2 {
                        return Err("Использование: /time set <0..24|day|noon|sunset|night>".to_string());
                    }
                    let target_time = match args[1].to_ascii_lowercase().as_str() {
                        "day" | "morning" => 8.0,
                        "noon" | "midday" => 12.0,
                        "sunset" | "dusk" => 18.5,
                        "night" | "midnight" => 0.0,
                        other => other
                            .parse::<f32>()
                            .map_err(|_| format!("Неверное время '{other}'"))?,
                    };
                    host.set_time_of_day(target_time);
                    let hours = target_time.rem_euclid(24.0).floor() as u32;
                    let minutes = ((target_time.rem_euclid(24.0) - hours as f32) * 60.0).floor() as u32;
                    Ok(format!("Время установлено на {:02}:{:02}", hours, minutes))
                }
                "add" => {
                    if args.len() < 2 {
                        return Err("Использование: /time add <часы>".to_string());
                    }
                    let delta = args[1]
                        .parse::<f32>()
                        .map_err(|_| format!("Неверное число '{}'", args[1]))?;
                    let new_time = host.time_of_day() + delta;
                    host.set_time_of_day(new_time);
                    let cur = host.time_of_day();
                    let hours = cur.floor() as u32;
                    let minutes = ((cur - hours as f32) * 60.0).floor() as u32;
                    Ok(format!("Время смещено на {:+0.1} ч (текущее: {:02}:{:02})", delta, hours, minutes))
                }
                "query" => {
                    let cur = host.time_of_day();
                    let hours = cur.floor() as u32;
                    let minutes = ((cur - hours as f32) * 60.0).floor() as u32;
                    Ok(format!("Текущее время: {:02}:{:02} ({:.2} ч)", hours, minutes, cur))
                }
                other => Err(format!(
                    "Неизвестное действие '{other}'. Доступно: set, add, query"
                )),
            }
        }
,
    );

    cmd.register(
        "materials",
        &["mat", "mats"],
        "/materials [фильтр]",
        "Список физических PBR материалов движка",
        |args, host| {
            let mats = host.material_registry();
            let filter = args.first().map(|s| s.to_lowercase());
            let list: Vec<_> = mats
                .iter()
                .filter(|m| {
                    if let Some(ref f) = filter {
                        m.name.to_lowercase().contains(f)
                    } else {
                        true
                    }
                })
                .map(|m| {
                    format!(
                        "• {} [шероховатость: {:.2}, металл: {:.2}, подповерхность: {:.2}]",
                        m.name, m.roughness, m.metallic, m.subsurface
                    )
                })
                .collect();
            Ok(format!("Материалы ({}/{}):\n{}", list.len(), mats.len(), list.join("\n")))
        },
    );
}

impl ApplicationHandler for EngineApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(format!("Voxel Engine [{}]", self.mode.id()))
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );

        let mut renderer = Renderer::new(window.clone(), self.settings.vsync);
        let initial_meshes = self
            .world
            .rebuild_dirty_meshes(&self.registry, usize::MAX);
        renderer.upload_chunk_meshes(initial_meshes);

        let egui_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.egui_state = Some(egui_state);
        self.set_cursor_lock(self.screen == MenuScreen::Playing);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Handle Escape before egui can swallow it in TextEdit, and always allow Redraw/Resize/Close.
        if let WindowEvent::KeyboardInput {
            event: ref key_ev, ..
        } = event
        {
            if key_ev.state == ElementState::Pressed
                && !key_ev.repeat
                && key_ev.physical_key == PhysicalKey::Code(KeyCode::Escape)
            {
                if self.rebinding.is_some() {
                    self.rebinding = None;
                } else if self.inventory_open {
                    self.inventory_open = false;
                    self.set_cursor_lock(true);
                    if let Some(held) = self.held_stack.take() {
                        self.inventory.add_item(held.block_id, held.count);
                    }
                } else if self.chat_open {
                    self.close_chat();
                } else {
                    match self.screen {
                        MenuScreen::Title => {}
                        MenuScreen::SelectWorld => self.screen = MenuScreen::Title,
                        MenuScreen::CreateWorld => self.screen = MenuScreen::SelectWorld,
                        MenuScreen::Playing => self.open_pause(),
                        MenuScreen::Settings | MenuScreen::HostServer | MenuScreen::JoinServer => {
                            self.screen = self.return_screen;
                        }
                        MenuScreen::Pause => self.resume_game(),
                    }
                }
                return;
            }
        }


        // Forward window events to egui whenever a menu, inventory, or chat is active
        if self.screen != MenuScreen::Playing || self.chat_open || self.inventory_open {
            if let (Some(window), Some(egui_state)) = (&self.window, &mut self.egui_state) {
                let response = egui_state.on_window_event(window, &event);
                let is_system_event = matches!(
                    event,
                    WindowEvent::RedrawRequested
                        | WindowEvent::Resized(_)
                        | WindowEvent::CloseRequested
                );
                if response.consumed && !is_system_event {
                    return;
                }
            }
        }


        match event {
            WindowEvent::CloseRequested => {
                self.save_current_world();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
            }
            WindowEvent::Focused(false) => {
                if self.screen == MenuScreen::Playing {
                    self.open_pause();
                } else {
                    self.input.keys.clear();
                    self.set_cursor_lock(false);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.screen == MenuScreen::Playing && !self.chat_open && !self.inventory_open {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    if scroll > 0.0 {
                        self.inventory.selected_hotbar = (self.inventory.selected_hotbar + 9 - 1) % 9;
                    } else if scroll < 0.0 {
                        self.inventory.selected_hotbar = (self.inventory.selected_hotbar + 1) % 9;
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !self.inventory_open {
                    if button == MouseButton::Left {
                        self.lmb_held = state == ElementState::Pressed;
                    }
                    if state == ElementState::Pressed {
                        self.handle_mouse_click(button);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if let Some(target) = self.rebinding {
                                if code == KeyCode::Escape {
                                    self.rebinding = None;
                                } else if let Some(name) = keycode_to_name(code) {
                                    let slot = match target {
                                        RebindTarget::Forward => &mut self.settings.keybinds.forward,
                                        RebindTarget::Backward => &mut self.settings.keybinds.backward,
                                        RebindTarget::Left => &mut self.settings.keybinds.left,
                                        RebindTarget::Right => &mut self.settings.keybinds.right,
                                        RebindTarget::JumpUp => &mut self.settings.keybinds.jump_up,
                                        RebindTarget::Down => &mut self.settings.keybinds.down,
                                        RebindTarget::Sprint => &mut self.settings.keybinds.sprint,
                                        RebindTarget::ToggleFly => &mut self.settings.keybinds.toggle_fly,
                                        RebindTarget::Chat => &mut self.settings.keybinds.chat,
                                        RebindTarget::Command => &mut self.settings.keybinds.command,
                                    };
                                    *slot = name.to_string();
                                    self.rebinding = None;
                                    self.settings.save();
                                }
                                return;
                            }

                            if code == KeyCode::KeyE && !event.repeat && self.screen == MenuScreen::Playing && !self.chat_open {
                                self.inventory_open = !self.inventory_open;
                                self.set_cursor_lock(!self.inventory_open);
                                if !self.inventory_open {
                                    if let Some(held) = self.held_stack.take() {
                                        self.inventory.add_item(held.block_id, held.count);
                                    }
                                }
                                return;
                            }

                            if code == KeyCode::Escape && !event.repeat {
                                if self.inventory_open {
                                    self.inventory_open = false;
                                    self.set_cursor_lock(true);
                                    if let Some(held) = self.held_stack.take() {
                                        self.inventory.add_item(held.block_id, held.count);
                                    }
                                    return;
                                }
                                if self.chat_open {
                                    self.close_chat();
                                    return;
                                }
                                match self.screen {
                                    MenuScreen::Title => {}
                                    MenuScreen::SelectWorld => self.screen = MenuScreen::Title,
                                    MenuScreen::CreateWorld => self.screen = MenuScreen::SelectWorld,
                                    MenuScreen::Playing => self.open_pause(),
                                    MenuScreen::Settings | MenuScreen::HostServer | MenuScreen::JoinServer => {
                                        self.screen = self.return_screen;
                                    }
                                    MenuScreen::Pause => self.resume_game(),
                                }
                                return;
                            }

                            if self.chat_open {
                                if code == KeyCode::Enter && !event.repeat {
                                    self.submit_chat();
                                }
                                return;
                            }

                            if self.screen == MenuScreen::Playing && !self.inventory_open && !event.repeat {
                                // Hotbar slot numbers 1..9
                                let slot_digit = match code {
                                    KeyCode::Digit1 => Some(0),
                                    KeyCode::Digit2 => Some(1),
                                    KeyCode::Digit3 => Some(2),
                                    KeyCode::Digit4 => Some(3),
                                    KeyCode::Digit5 => Some(4),
                                    KeyCode::Digit6 => Some(5),
                                    KeyCode::Digit7 => Some(6),
                                    KeyCode::Digit8 => Some(7),
                                    KeyCode::Digit9 => Some(8),
                                    _ => None,
                                };
                                if let Some(slot) = slot_digit {
                                    self.inventory.selected_hotbar = slot;
                                }

                                // Drop held item on Q
                                if code == KeyCode::KeyQ && !event.repeat {
                                    let sel = self.inventory.selected_hotbar;
                                    if let Some(slot) = &mut self.inventory.slots[sel] {
                                        let drop_block = slot.block_id;
                                        if slot.count <= 1 {
                                            self.inventory.slots[sel] = None;
                                        } else {
                                            slot.count -= 1;
                                        }
                                        let fwd = self.camera.forward();
                                        let drop_pos = self.camera.position + fwd * 0.6;
                                        let drop_vel = fwd * 4.8 + Vec3::new(0.0, 1.8, 0.0);
                                        self.spawn_dropped_item(drop_block, 1, drop_pos, drop_vel);
                                        audio::play_event_at("item.drop", drop_pos);
                                    }
                                }

                                // Double-space in creative mode toggles flying
                                if name_to_keycode(&self.settings.keybinds.jump_up) == Some(code) {
                                    self.jump_just_pressed = true;
                                    let now = Instant::now();
                                    if let Some(last) = self.last_space_press {
                                        if now.duration_since(last).as_secs_f32() < 0.28 {
                                            if self.player_mode == PlayerMode::Creative
                                                || self.allow_fly
                                            {
                                                self.toggle_flight_mode();
                                            }
                                            self.last_space_press = None;
                                        } else {
                                            self.last_space_press = Some(now);
                                        }
                                    } else {
                                        self.last_space_press = Some(now);
                                    }
                                }

                                // Open Chat or Command bar
                                if name_to_keycode(&self.settings.keybinds.chat) == Some(code) {
                                    self.open_chat("");
                                    return;
                                }
                                if name_to_keycode(&self.settings.keybinds.command) == Some(code) {
                                    self.open_chat("/");
                                    return;
                                }

                                if name_to_keycode(&self.settings.keybinds.toggle_fly) == Some(code)
                                {
                                    self.toggle_flight_mode();
                                }

                                if code == KeyCode::F5 && !event.repeat {
                                    self.perspective = match self.perspective {
                                        CameraPerspective::FirstPerson => CameraPerspective::ThirdPersonBack,
                                        CameraPerspective::ThirdPersonBack => CameraPerspective::ThirdPersonFront,
                                        CameraPerspective::ThirdPersonFront => CameraPerspective::FirstPerson,
                                    };
                                    let label = self.perspective.label_ru();
                                    self.push_chat(
                                        format!("Вид камеры: {label}"),
                                        egui::Color32::from_rgb(255, 215, 120),
                                    );
                                }

                                // ===== [TEMPORARY PHYSICS TEST: START] =====
                                if (code == KeyCode::KeyG || code == KeyCode::KeyU) && !event.repeat {
                                    let held_id = self.inventory.selected_block();
                                    let spawn_block_id = if held_id == BlockId::AIR { BlockId(1) } else { held_id };

                                    let spawn_origin = self.camera.position + self.camera.forward() * 2.0 - Vec3::new(0.5, 0.5, 0.5);
                                    let impulse = self.camera.forward() * 7.5 + Vec3::new(0.0, 3.2, 0.0);
                                    let mut transform = RelativeTransform::new(ReferenceFrameId::UNIVERSE_ROOT, IVec3::ZERO, spawn_origin);
                                    transform.local_velocity = impulse;
                                    let mut contraption = VoxelContraption::new(
                                        (self.test_physics_blocks.len() + 1) as u64,
                                        transform,
                                        1.0,
                                    );
                                    contraption.set_block(IVec3::ZERO, spawn_block_id);
                                    contraption.rebuild_meshes(&self.registry);
                                    contraption.rebuild_colliders();

                                    self.test_physics_blocks.push(contraption);
                                    if self.test_physics_blocks.len() > 32 {
                                        self.test_physics_blocks.remove(0);
                                    }

                                    audio::play_sound(audio::SoundEffect::FootstepStone);
                                    let block_name = self.registry.get(spawn_block_id).map(|d| d.name.as_str()).unwrap_or("block");
                                    self.push_chat(
                                        format!("Брошен физический блок: {block_name} (нажмите G или Г для повтора)"),
                                        egui::Color32::from_rgb(100, 220, 255),
                                    );
                                }
                                // ===== [TEMPORARY PHYSICS TEST: END] =====
                            }
                            self.input.keys.insert(code);
                        }
                        ElementState::Released => {
                            self.input.keys.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32();
                self.last_frame = now;

                self.update(dt);

                let full_output = self.build_ui(event_loop);
                if let (Some(window), Some(egui_state)) = (&self.window, &mut self.egui_state) {
                    egui_state.handle_platform_output(window, full_output.platform_output);
                }

                let paint_jobs = self
                    .egui_ctx
                    .tessellate(full_output.shapes, full_output.pixels_per_point);

                let targeted_block = if self.screen == MenuScreen::Playing && !self.chat_open && !self.inventory_open {
                    self.world
                        .raycast(self.camera.position, self.camera.forward(), 6.5, &self.registry)
                        .map(|h| h.hit_pos)
                } else {
                    None
                };

                let render_cam = match self.perspective {
                    CameraPerspective::FirstPerson => self.camera,
                    CameraPerspective::ThirdPersonBack => {
                        let target = self.player_feet() + Vec3::Y * (PLAYER_EYE_HEIGHT - 0.1);
                        let back_dir = -self.camera.forward();
                        let max_dist = 3.2_f32;
                        let hit_dist = self
                            .world
                            .raycast(target, back_dir, max_dist, &self.registry)
                            .map(|h| {
                                let block_center = h.hit_pos.as_vec3() + Vec3::splat(0.5);
                                (target.distance(block_center) - 0.40).max(0.6)
                            })
                            .unwrap_or(max_dist);
                        let mut cam = self.camera;
                        cam.position =
                            target + back_dir * hit_dist + self.camera.right_xz() * 0.32 + Vec3::Y * 0.18;
                        cam
                    }
                    CameraPerspective::ThirdPersonFront => {
                        let target = self.player_feet() + Vec3::Y * (PLAYER_EYE_HEIGHT - 0.1);
                        let front_dir = self.camera.forward();
                        let max_dist = 2.4_f32;
                        let hit_dist = self
                            .world
                            .raycast(target, front_dir, max_dist, &self.registry)
                            .map(|h| {
                                let block_center = h.hit_pos.as_vec3() + Vec3::splat(0.5);
                                (target.distance(block_center) - 0.40).max(0.6)
                            })
                            .unwrap_or(max_dist);
                        let mut cam = self.camera;
                        cam.position = target + front_dir * hit_dist + Vec3::Y * 0.12;
                        cam.yaw = self.camera.yaw + std::f32::consts::PI;
                        cam.pitch = -self.camera.pitch;
                        cam
                    }
                };

                let mut render_cam = render_cam;
                if self.death_fall_progress > 0.0 {
                    render_cam.position.y -= self.death_fall_progress * 1.15;
                    render_cam.pitch = (render_cam.pitch + self.death_fall_progress * 0.65).clamp(-1.54, 1.54);
                }
                if self.camera_flinch > 0.001 {
                    render_cam.pitch -= self.camera_flinch * 0.5;
                }

                let mut all_avatar_verts = Vec::new();

                if self.screen != MenuScreen::Title
                    && self.screen != MenuScreen::SelectWorld
                    && self.screen != MenuScreen::CreateWorld
                    && self.screen != MenuScreen::HostServer
                    && self.screen != MenuScreen::JoinServer
                {
                    let held_visuals = if let Some(item) = self.inventory.selected_item() {
                        if item.block_id != BlockId::AIR {
                            self.registry.get(item.block_id).map(|d| (d.face_colors, d.face_textures))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let held_colors = held_visuals.map(|(c, _)| c);
                    let held_textures = held_visuals.map(|(_, t)| t);

                    if self.perspective != CameraPerspective::FirstPerson {
                        let horiz_speed = Vec3::new(self.velocity.x, 0.0, self.velocity.z).length();
                        let verts = build_avatar_mesh(
                            &self.avatar,
                            self.player_feet(),
                            self.camera.yaw,
                            self.camera.pitch,
                            self.walk_phase,
                            horiz_speed > 0.15,
                            horiz_speed,
                            self.controller.is_crouching,
                            !self.controller.on_ground && !self.controller.in_water,
                            self.controller.in_water,
                            self.arm_swing_progress,
                            held_colors,
                            held_textures,
                        );
                        all_avatar_verts.extend(verts);
                    } else {
                        let horiz_speed = Vec3::new(self.velocity.x, 0.0, self.velocity.z).length();
                        let verts = build_first_person_arm_mesh(
                            &self.avatar,
                            self.camera.position,
                            self.camera.yaw,
                            self.camera.pitch,
                            self.walk_phase,
                            horiz_speed > 0.15,
                            self.arm_swing_progress,
                            self.hand_sway,
                            held_colors,
                            held_textures,
                        );
                        all_avatar_verts.extend(verts);
                    }
                }

                // Render remote players
                let remote_players: Vec<RemotePlayer> = if self.server_host.is_some() {
                    self.host_remote_players.values().cloned().collect()
                } else if let Some(client) = &self.game_client {
                    client.remote_players.values().cloned().collect()
                } else {
                    Vec::new()
                };

                for remote in &remote_players {
                    let remote_visuals = if remote.held_block != 0 {
                        self.registry
                            .get(BlockId(remote.held_block))
                            .map(|d| (d.face_colors, d.face_textures))
                    } else {
                        None
                    };
                    let remote_colors = remote_visuals.map(|(c, _)| c);
                    let remote_textures = remote_visuals.map(|(_, t)| t);
                    let is_dead = remote.health <= 0.0;
                    let render_pos = if is_dead {
                        remote.current_pos - Vec3::new(0.0, 0.75, 0.0)
                    } else {
                        remote.current_pos
                    };
                    let render_pitch = if is_dead {
                        -std::f32::consts::FRAC_PI_2
                    } else {
                        remote.current_pitch
                    };
                    let mut verts = build_avatar_mesh(
                        &self.avatar,
                        render_pos,
                        remote.current_yaw,
                        render_pitch,
                        if is_dead { 0.0 } else { remote.walk_phase },
                        if is_dead { false } else { remote.is_moving },
                        if is_dead || !remote.is_moving { 0.0 } else { 4.5 },
                        false,
                        false,
                        false,
                        if is_dead { 0.0 } else { remote.arm_swing },
                        if is_dead { None } else { remote_colors },
                        if is_dead { None } else { remote_textures },
                    );
                    if remote.hurt_timer > 0.0 || is_dead {
                        for v in &mut verts {
                            v.color[0] = 0.95;
                            v.color[1] = v.color[1] * 0.25;
                            v.color[2] = v.color[2] * 0.25;
                        }
                    }
                    all_avatar_verts.extend(verts);
                }

                // Render dropped items in world
                for item in &self.dropped_items {
                    if let Some(def) = self.registry.get(item.block_id) {
                        let bob = (item.age * 3.0).sin() * 0.05;
                        let rot_angle = item.age * 2.0;
                        let rot = Mat3::from_rotation_y(rot_angle);
                        let center = item.position + Vec3::new(0.0, 0.15 + bob, 0.0);
                        let half_size = Vec3::splat(0.12);
                        push_textured_box(
                            &mut all_avatar_verts,
                            center,
                            half_size,
                            rot,
                            def.face_colors,
                            def.face_textures,
                        );
                    }
                }

                // ===== [TEMPORARY PHYSICS TEST: START] =====
                for block in &self.test_physics_blocks {
                    for (_pos, mesh) in &block.meshes {
                        for &idx in &mesh.indices {
                            if let Some(v_orig) = mesh.vertices.get(idx as usize) {
                                let mut v = *v_orig;
                                let local_pt = Vec3::from_array(v.position);
                                v.position = block.transform_point_to_world(local_pt).to_array();
                                let world_norm = block.transform.local_rotation * Vec3::from_array(v.normal);
                                v.normal = world_norm.to_array();
                                all_avatar_verts.push(v);
                            }
                        }
                    }
                }
                // ===== [TEMPORARY PHYSICS TEST: END] =====

                let avatar_verts = if all_avatar_verts.is_empty() {
                    None
                } else {
                    Some(all_avatar_verts)
                };

                let env_state = self.environment.evaluate(self.time_of_day);
                let fog_dist = (self.settings.render_distance as f32) * (CHUNK_SIZE_I32 as f32) * env_state.fog_density;
                if let Some(renderer) = &mut self.renderer {
                    match renderer.render(
                        &render_cam,
                        fog_dist,
                        self.settings.brightness,
                        &env_state,
                        avatar_verts.as_deref(),
                        targeted_block,
                        full_output.pixels_per_point,
                        &paint_jobs,
                        &full_output.textures_delta,
                    ) {
                        Ok(()) => {}
                        Err(RenderError::ResizeNeeded) => {
                            if let Some(win) = &self.window {
                                renderer.resize(win.inner_size());
                            }
                        }
                        Err(RenderError::OutOfMemory) => event_loop.exit(),
                        Err(RenderError::Other(msg)) => eprintln!("render error: {msg}"),
                    }
                }

                self.frame_counter += 1;
                let elapsed = (now - self.fps_timer).as_secs_f32();
                if elapsed >= 0.5 {
                    self.displayed_fps = (self.frame_counter as f32 / elapsed).round() as u32;
                    self.frame_counter = 0;
                    self.fps_timer = now;
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.screen == MenuScreen::Playing && !self.chat_open && self.input.cursor_locked {
                self.input.mouse_delta.0 += dx as f32;
                self.input.mouse_delta.1 += dy as f32;
            }
        }

    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn setup_cyrillic_font(ctx: &egui::Context) {
    let font_paths = [
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arial.ttf",
        r"C:\Windows\Fonts\tahoma.ttf",
    ];
    for path in font_paths {
        if let Ok(bytes) = fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "system_cyrillic".to_string(),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "system_cyrillic".to_string());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("system_cyrillic".to_string());
            ctx.set_fonts(fonts);
            return;
        }
    }
}

fn block_intersects_player(block: IVec3, feet: Vec3) -> bool {
    let p_min = Vec3::new(feet.x - PLAYER_RADIUS, feet.y, feet.z - PLAYER_RADIUS);
    let p_max = Vec3::new(
        feet.x + PLAYER_RADIUS,
        feet.y + PLAYER_HEIGHT,
        feet.z + PLAYER_RADIUS,
    );
    let b_min = block.as_vec3();
    let b_max = b_min + Vec3::ONE;
    p_min.x < b_max.x
        && p_max.x > b_min.x
        && p_min.y < b_max.y
        && p_max.y > b_min.y
        && p_min.z < b_max.z
        && p_max.z > b_min.z
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = EngineApp::new(Box::new(SurvivalMode::default()));
    event_loop.run_app(&mut app).expect("event loop error");
}
