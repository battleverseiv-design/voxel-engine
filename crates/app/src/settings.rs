use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use winit::keyboard::KeyCode;

const SETTINGS_FILE: &str = "settings.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBinds {
    pub forward: String,
    pub backward: String,
    pub left: String,
    pub right: String,
    pub jump_up: String,
    pub down: String,
    pub sprint: String,
    pub toggle_fly: String,
    pub chat: String,
    pub command: String,
}

impl Default for KeyBinds {
    fn default() -> Self {
        Self {
            forward: "W".to_string(),
            backward: "S".to_string(),
            left: "A".to_string(),
            right: "D".to_string(),
            jump_up: "Space".to_string(),
            down: "LCtrl".to_string(),
            sprint: "LShift".to_string(),
            toggle_fly: "F".to_string(),
            chat: "T".to_string(),
            command: "/".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mouse_sensitivity: f32,
    pub mouse_smoothing: f32,
    pub invert_y: bool,
    pub fov_degrees: f32,
    pub render_distance: i32,
    pub brightness: f32,
    pub vsync: bool,
    pub fly_speed: f32,
    pub show_hud: bool,
    pub player_name: String,
    pub last_connected_ip: String,
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub ambient_volume: f32,
    pub ui_volume: f32,
    pub keybinds: KeyBinds,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.0,
            mouse_smoothing: 0.15,
            invert_y: false,
            fov_degrees: 75.0,
            render_distance: 5,
            brightness: 1.0,
            vsync: false,
            fly_speed: 14.0,
            show_hud: true,
            player_name: "Player".to_string(),
            last_connected_ip: "127.0.0.1:25565".to_string(),
            master_volume: 1.0,
            sfx_volume: 1.0,
            ambient_volume: 0.8,
            ui_volume: 1.0,
            keybinds: KeyBinds::default(),
        }
    }
}

impl Settings {
    pub fn file_path() -> PathBuf {
        PathBuf::from(SETTINGS_FILE)
    }

    pub fn load() -> Self {
        let path = Self::file_path();
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        if let Ok(text) = toml::to_string_pretty(self) {
            let _ = fs::write(Self::file_path(), text);
        }
    }
}

pub fn keycode_to_name(code: KeyCode) -> Option<&'static str> {
    match code {
        KeyCode::KeyA => Some("A"),
        KeyCode::KeyB => Some("B"),
        KeyCode::KeyC => Some("C"),
        KeyCode::KeyD => Some("D"),
        KeyCode::KeyE => Some("E"),
        KeyCode::KeyF => Some("F"),
        KeyCode::KeyG => Some("G"),
        KeyCode::KeyH => Some("H"),
        KeyCode::KeyI => Some("I"),
        KeyCode::KeyJ => Some("J"),
        KeyCode::KeyK => Some("K"),
        KeyCode::KeyL => Some("L"),
        KeyCode::KeyM => Some("M"),
        KeyCode::KeyN => Some("N"),
        KeyCode::KeyO => Some("O"),
        KeyCode::KeyP => Some("P"),
        KeyCode::KeyQ => Some("Q"),
        KeyCode::KeyR => Some("R"),
        KeyCode::KeyS => Some("S"),
        KeyCode::KeyT => Some("T"),
        KeyCode::KeyU => Some("U"),
        KeyCode::KeyV => Some("V"),
        KeyCode::KeyW => Some("W"),
        KeyCode::KeyX => Some("X"),
        KeyCode::KeyY => Some("Y"),
        KeyCode::KeyZ => Some("Z"),
        KeyCode::Digit0 => Some("0"),
        KeyCode::Digit1 => Some("1"),
        KeyCode::Digit2 => Some("2"),
        KeyCode::Digit3 => Some("3"),
        KeyCode::Digit4 => Some("4"),
        KeyCode::Digit5 => Some("5"),
        KeyCode::Digit6 => Some("6"),
        KeyCode::Digit7 => Some("7"),
        KeyCode::Digit8 => Some("8"),
        KeyCode::Digit9 => Some("9"),
        KeyCode::Space => Some("Space"),
        KeyCode::ShiftLeft => Some("LShift"),
        KeyCode::ShiftRight => Some("RShift"),
        KeyCode::ControlLeft => Some("LCtrl"),
        KeyCode::ControlRight => Some("RCtrl"),
        KeyCode::AltLeft => Some("LAlt"),
        KeyCode::Tab => Some("Tab"),
        KeyCode::Slash => Some("/"),
        KeyCode::Enter => Some("Enter"),
        KeyCode::ArrowUp => Some("Up"),
        KeyCode::ArrowDown => Some("Down"),
        KeyCode::ArrowLeft => Some("Left"),
        KeyCode::ArrowRight => Some("Right"),
        _ => None,
    }
}

pub fn name_to_keycode(name: &str) -> Option<KeyCode> {
    match name {
        "A" => Some(KeyCode::KeyA),
        "B" => Some(KeyCode::KeyB),
        "C" => Some(KeyCode::KeyC),
        "D" => Some(KeyCode::KeyD),
        "E" => Some(KeyCode::KeyE),
        "F" => Some(KeyCode::KeyF),
        "G" => Some(KeyCode::KeyG),
        "H" => Some(KeyCode::KeyH),
        "I" => Some(KeyCode::KeyI),
        "J" => Some(KeyCode::KeyJ),
        "K" => Some(KeyCode::KeyK),
        "L" => Some(KeyCode::KeyL),
        "M" => Some(KeyCode::KeyM),
        "N" => Some(KeyCode::KeyN),
        "O" => Some(KeyCode::KeyO),
        "P" => Some(KeyCode::KeyP),
        "Q" => Some(KeyCode::KeyQ),
        "R" => Some(KeyCode::KeyR),
        "S" => Some(KeyCode::KeyS),
        "T" => Some(KeyCode::KeyT),
        "U" => Some(KeyCode::KeyU),
        "V" => Some(KeyCode::KeyV),
        "W" => Some(KeyCode::KeyW),
        "X" => Some(KeyCode::KeyX),
        "Y" => Some(KeyCode::KeyY),
        "Z" => Some(KeyCode::KeyZ),
        "0" => Some(KeyCode::Digit0),
        "1" => Some(KeyCode::Digit1),
        "2" => Some(KeyCode::Digit2),
        "3" => Some(KeyCode::Digit3),
        "4" => Some(KeyCode::Digit4),
        "5" => Some(KeyCode::Digit5),
        "6" => Some(KeyCode::Digit6),
        "7" => Some(KeyCode::Digit7),
        "8" => Some(KeyCode::Digit8),
        "9" => Some(KeyCode::Digit9),
        "Space" => Some(KeyCode::Space),
        "LShift" => Some(KeyCode::ShiftLeft),
        "RShift" => Some(KeyCode::ShiftRight),
        "LCtrl" => Some(KeyCode::ControlLeft),
        "RCtrl" => Some(KeyCode::ControlRight),
        "LAlt" => Some(KeyCode::AltLeft),
        "Tab" => Some(KeyCode::Tab),
        "/" => Some(KeyCode::Slash),
        "Enter" => Some(KeyCode::Enter),
        "Up" => Some(KeyCode::ArrowUp),
        "Down" => Some(KeyCode::ArrowDown),
        "Left" => Some(KeyCode::ArrowLeft),
        "Right" => Some(KeyCode::ArrowRight),
        _ => None,
    }
}
