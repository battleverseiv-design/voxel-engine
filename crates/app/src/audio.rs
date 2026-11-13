//! Sound effect and spatial audio bridge for Voxel Engine.
//! Powered by `engine-audio` with 3D positional audio, multi-channel mixing,
//! smooth ambient crossfading, and data-driven sound events.

#![allow(dead_code)]

use std::sync::OnceLock;
pub use engine_audio::{AudioEngine, SoundCategory};
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundEffect {
    Footstep,
    FootstepGrass,
    FootstepDirt,
    FootstepStone,
    FootstepWood,
    FootstepSand,
    FootstepGravel,
    FootstepLeaves,
    Jump,
    BlockBreak,
    BlockPlace,
    Hit,
    Crit,
    Hurt,
    Death,
    Respawn,
    Heal,
    Teleport,
}

static AUDIO_ENGINE: OnceLock<AudioEngine> = OnceLock::new();

pub fn engine() -> &'static AudioEngine {
    AUDIO_ENGINE.get_or_init(|| {
        let engine = AudioEngine::new();
        register_default_sounds(&engine);
        engine
    })
}

fn register_default_sounds(engine: &AudioEngine) {
    // 1. Block Break (Material-aware from Enhanced Audio)
    engine.register_simple_event(
        "block.break.grass",
        SoundCategory::Sfx,
        &["break_grass_1.wav", "break_grass_2.wav", "break_grass_3.wav", "break_grass_4.wav"],
        0.9,
        32.0,
    );
    engine.register_simple_event(
        "block.break.stone",
        SoundCategory::Sfx,
        &["break_stone_1.wav", "break_stone_2.wav", "break_stone_3.wav", "break_stone_4.wav"],
        0.95,
        32.0,
    );
    engine.register_simple_event(
        "block.break.wood",
        SoundCategory::Sfx,
        &["break_wood_1.wav", "break_wood_2.wav", "break_wood_3.wav", "break_wood_4.wav"],
        0.9,
        32.0,
    );
    engine.register_simple_event(
        "block.break.dirt",
        SoundCategory::Sfx,
        &["break_dirt_1.wav", "break_dirt_2.wav", "break_dirt_3.wav", "break_dirt_4.wav"],
        0.85,
        32.0,
    );
    engine.register_simple_event(
        "block.break.sand",
        SoundCategory::Sfx,
        &["break_sand_1.wav", "break_sand_2.wav", "break_sand_3.wav", "break_sand_4.wav"],
        0.85,
        32.0,
    );
    engine.register_simple_event(
        "block.break.glass",
        SoundCategory::Sfx,
        &["break_glass_1.wav", "break_glass_2.wav", "break_glass_3.wav", "break_glass_4.wav"],
        1.0,
        32.0,
    );
    engine.register_simple_event(
        "block.break.iron",
        SoundCategory::Sfx,
        &["break_iron_1.wav", "break_iron_2.wav", "break_iron_3.wav", "break_iron_4.wav"],
        0.9,
        32.0,
    );
    engine.register_simple_event(
        "block.break",
        SoundCategory::Sfx,
        &["break_stone_1.wav", "break.wav"],
        0.9,
        32.0,
    );

    // 2. Block Place & Mining
    engine.register_simple_event(
        "block.place",
        SoundCategory::Sfx,
        &["place_1.wav", "place_2.wav", "place_3.wav", "place_4.wav", "pop_1.wav"],
        0.85,
        32.0,
    );
    engine.register_simple_event(
        "mine.stone",
        SoundCategory::Sfx,
        &["mine_stone_1.wav", "mine_stone_2.wav"],
        0.8,
        28.0,
    );
    engine.register_simple_event(
        "mine.wood",
        SoundCategory::Sfx,
        &["mine_wood_1.wav", "mine_wood_2.wav"],
        0.8,
        28.0,
    );
    engine.register_simple_event(
        "mine.dirt",
        SoundCategory::Sfx,
        &["mine_dirt_1.wav", "mine_dirt_2.wav"],
        0.75,
        28.0,
    );

    // 3. Footsteps (Material-aware from Presence Footsteps)
    engine.register_simple_event(
        "footstep.grass",
        SoundCategory::Sfx,
        &["step_grass_1.wav", "step_grass_2.wav", "step_grass_3.wav", "step_grass_4.wav"],
        0.75,
        24.0,
    );
    engine.register_simple_event(
        "footstep.dirt",
        SoundCategory::Sfx,
        &["step_dirt_1.wav", "step_dirt_2.wav", "step_dirt_3.wav", "step_dirt_4.wav"],
        0.75,
        24.0,
    );
    engine.register_simple_event(
        "footstep.stone",
        SoundCategory::Sfx,
        &["step_stone_1.wav", "step_stone_2.wav", "step_stone_3.wav", "step_stone_4.wav"],
        0.8,
        24.0,
    );
    engine.register_simple_event(
        "footstep.wood",
        SoundCategory::Sfx,
        &["step_wood_1.wav", "step_wood_2.wav", "step_wood_3.wav", "step_wood_4.wav"],
        0.8,
        24.0,
    );
    engine.register_simple_event(
        "footstep.sand",
        SoundCategory::Sfx,
        &["step_sand_1.wav", "step_sand_2.wav", "step_sand_3.wav", "step_sand_4.wav"],
        0.75,
        24.0,
    );
    engine.register_simple_event(
        "footstep.gravel",
        SoundCategory::Sfx,
        &["step_gravel_1.wav", "step_gravel_2.wav", "step_gravel_3.wav", "step_gravel_4.wav"],
        0.8,
        24.0,
    );
    engine.register_simple_event(
        "footstep.leaves",
        SoundCategory::Sfx,
        &["step_leaves_1.wav", "step_leaves_2.wav", "step_leaves_3.wav", "step_leaves_4.wav"],
        0.7,
        24.0,
    );
    engine.register_simple_event(
        "footstep",
        SoundCategory::Sfx,
        &["step_grass_1.wav", "step.wav"],
        0.75,
        24.0,
    );

    // 4. Combat & Movement
    engine.register_simple_event(
        "combat.hit",
        SoundCategory::Sfx,
        &["hit_1.wav", "hit_2.wav", "hit_3.wav"],
        0.95,
        36.0,
    );
    engine.register_simple_event(
        "combat.crit",
        SoundCategory::Sfx,
        &["levelup.wav"],
        1.0,
        36.0,
    );
    engine.register_simple_event(
        "player.hurt",
        SoundCategory::Sfx,
        &["hurt.wav", "hurt_big_1.wav", "hurt_big_2.wav"],
        0.95,
        32.0,
    );
    engine.register_simple_event(
        "player.death",
        SoundCategory::Sfx,
        &["death_1.wav", "death_2.wav"],
        1.0,
        48.0,
    );
    engine.register_simple_event(
        "player.jump",
        SoundCategory::Sfx,
        &["dash.wav", "pop_2.wav"],
        0.7,
        24.0,
    );
    engine.register_simple_event(
        "player.heal",
        SoundCategory::Sfx,
        &["levelup.wav"],
        0.9,
        32.0,
    );
    engine.register_simple_event(
        "player.teleport",
        SoundCategory::Sfx,
        &["pop_3.wav"],
        0.95,
        36.0,
    );

    // 5. Ambient Loops (from AmbientSounds mod)
    engine.register_simple_event(
        "ambient.day",
        SoundCategory::Ambient,
        &["ambient_day.wav"],
        0.45,
        100.0,
    );
    engine.register_simple_event(
        "ambient.night",
        SoundCategory::Ambient,
        &["ambient_night.wav"],
        0.5,
        100.0,
    );
    engine.register_simple_event(
        "ambient.cave",
        SoundCategory::Ambient,
        &["ambient_cave.wav"],
        0.65,
        100.0,
    );

    // 6. UI
    engine.register_simple_event(
        "ui.click",
        SoundCategory::Ui,
        &["ui_click.wav"],
        0.8,
        10.0,
    );
}

pub fn play_event(name: &str, pos: Option<Vec3>) {
    engine().play_event(name, pos);
}

pub fn play_event_at(name: &str, pos: Vec3) {
    engine().play_event(name, Some(pos));
}

pub fn play_ui(name: &str) {
    engine().play_ui(name);
}

pub fn set_listener(pos: Vec3, forward: Vec3, right: Vec3) {
    engine().set_listener(pos, forward, right);
}

pub fn set_ambient(name: &str) {
    engine().set_ambient(name);
}

pub fn stop_ambient() {
    engine().stop_ambient();
}

pub fn set_volume(category: SoundCategory, vol: f32) {
    engine().set_volume(category, vol);
}

/// Automatically adjusts ambient audio based on in-game time of day and player depth.
pub fn update_ambient(time_of_day: f32, player_y: f32) {
    if player_y < -4.0 {
        set_ambient("ambient.cave");
    } else if (6.0..18.5).contains(&time_of_day) {
        set_ambient("ambient.day");
    } else {
        set_ambient("ambient.night");
    }
}

/// Material-aware 3D block breaking sound.
pub fn play_block_break(block_name: &str, pos: Option<Vec3>) {
    let lower = block_name.to_lowercase();
    let event = if lower.contains("grass") {
        "block.break.grass"
    } else if lower.contains("stone")
        || lower.contains("cobble")
        || lower.contains("brick")
        || lower.contains("andesite")
        || lower.contains("furnace")
    {
        "block.break.stone"
    } else if lower.contains("wood")
        || lower.contains("planks")
        || lower.contains("bookshelf")
        || lower.contains("crafting")
    {
        "block.break.wood"
    } else if lower.contains("sand") {
        "block.break.sand"
    } else if lower.contains("dirt") {
        "block.break.dirt"
    } else if lower.contains("glass") {
        "block.break.glass"
    } else if lower.contains("iron") || lower.contains("gold") || lower.contains("diamond") {
        "block.break.iron"
    } else {
        "block.break"
    };
    play_event(event, pos);
}

/// Material-aware block hit / mining sound (during digging).
pub fn play_block_hit(block_name: &str, pos: Option<Vec3>) {
    let lower = block_name.to_lowercase();
    let event = if lower.contains("wood")
        || lower.contains("planks")
        || lower.contains("log")
        || lower.contains("crafting")
        || lower.contains("bookshelf")
    {
        "mine.wood"
    } else if lower.contains("dirt")
        || lower.contains("grass")
        || lower.contains("sand")
        || lower.contains("gravel")
        || lower.contains("clay")
        || lower.contains("leaves")
        || lower.contains("snow")
        || lower.contains("soul_sand")
    {
        "mine.dirt"
    } else {
        "mine.stone"
    };
    play_event(event, pos);
}

/// Compatibility bridge for existing calls.
pub fn play_sound(effect: SoundEffect) {
    match effect {
        SoundEffect::Footstep => play_event("footstep", None),
        SoundEffect::FootstepGrass => play_event("footstep.grass", None),
        SoundEffect::FootstepDirt => play_event("footstep.dirt", None),
        SoundEffect::FootstepStone => play_event("footstep.stone", None),
        SoundEffect::FootstepWood => play_event("footstep.wood", None),
        SoundEffect::FootstepSand => play_event("footstep.sand", None),
        SoundEffect::FootstepGravel => play_event("footstep.gravel", None),
        SoundEffect::FootstepLeaves => play_event("footstep.leaves", None),
        SoundEffect::Jump => play_event("player.jump", None),
        SoundEffect::BlockBreak => play_event("block.break", None),
        SoundEffect::BlockPlace => play_event("block.place", None),
        SoundEffect::Hit => play_event("combat.hit", None),
        SoundEffect::Crit => play_event("combat.crit", None),
        SoundEffect::Hurt => play_event("player.hurt", None),
        SoundEffect::Death => play_event("player.death", None),
        SoundEffect::Respawn => play_event("player.heal", None),
        SoundEffect::Heal => play_event("player.heal", None),
        SoundEffect::Teleport => play_event("player.teleport", None),
    }
}
