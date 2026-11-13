use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

const WORLDS_DIR: &str = "worlds";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMeta {
    pub id: String,
    pub name: String,
    pub seed: u32,
    pub mode: String,          // "Survival" or "Creative"
    pub generator: String,     // "Lithosphere" or "Classic"
    pub created_at: String,
    pub last_played: String,
    pub player_pos: [f32; 3],
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub player_health: f32,
}

impl Default for WorldMeta {
    fn default() -> Self {
        Self {
            id: "default_world".to_string(),
            name: "Новый мир".to_string(),
            seed: 42,
            mode: "Survival".to_string(),
            generator: "Lithosphere".to_string(),
            created_at: Self::current_timestamp_string(),
            last_played: Self::current_timestamp_string(),
            player_pos: [0.5, 42.0, 0.5],
            player_yaw: 0.0,
            player_pitch: 0.0,
            player_health: 100.0,
        }
    }
}

impl WorldMeta {
    pub fn current_timestamp_string() -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = now.as_secs();
        let days = secs / 86400;
        let rem_secs = secs % 86400;
        let hours = rem_secs / 3600;
        let mins = (rem_secs % 3600) / 60;
        format!("Day {days}, {hours:02}:{mins:02} UTC")
    }

    pub fn folder_path(&self) -> PathBuf {
        Path::new(WORLDS_DIR).join(&self.id)
    }

    pub fn meta_file_path(&self) -> PathBuf {
        self.folder_path().join("world.toml")
    }

    pub fn edits_file_path(&self) -> PathBuf {
        self.folder_path().join("edits.dat")
    }
}

pub struct WorldManager;

impl WorldManager {
    pub fn ensure_worlds_dir() {
        let _ = fs::create_dir_all(WORLDS_DIR);
    }

    /// Lists all worlds present in the `worlds/` directory, sorted newest first.
    pub fn list_worlds() -> Vec<WorldMeta> {
        Self::ensure_worlds_dir();
        let mut worlds = Vec::new();

        if let Ok(entries) = fs::read_dir(WORLDS_DIR) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let meta_file = path.join("world.toml");
                    if let Ok(content) = fs::read_to_string(&meta_file) {
                        if let Ok(meta) = toml::from_str::<WorldMeta>(&content) {
                            worlds.push(meta);
                        }
                    }
                }
            }
        }

        // Sort by id descending as fallback timestamp proxy
        worlds.sort_by(|a, b| b.id.cmp(&a.id));
        worlds
    }

    /// Creates a brand-new world directory with `world.toml`.
    pub fn create_world(
        name: &str,
        seed: u32,
        mode: &str,
        generator: &str,
        spawn_pos: [f32; 3],
    ) -> Result<WorldMeta, String> {
        Self::ensure_worlds_dir();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Create a filesystem-safe folder id
        let slug: String = name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        let clean_slug = if slug.is_empty() { "world" } else { &slug };
        let id = format!("{clean_slug}_{timestamp}");

        let meta = WorldMeta {
            id,
            name: name.trim().to_string(),
            seed,
            mode: mode.to_string(),
            generator: generator.to_string(),
            created_at: WorldMeta::current_timestamp_string(),
            last_played: WorldMeta::current_timestamp_string(),
            player_pos: spawn_pos,
            player_yaw: 0.0,
            player_pitch: 0.0,
            player_health: 100.0,
        };

        let folder = meta.folder_path();
        fs::create_dir_all(&folder).map_err(|e| format!("Failed to create world folder: {e}"))?;

        let toml_str = toml::to_string_pretty(&meta)
            .map_err(|e| format!("Failed to serialize world meta: {e}"))?;
        fs::write(meta.meta_file_path(), toml_str)
            .map_err(|e| format!("Failed to write world.toml: {e}"))?;

        Ok(meta)
    }

    /// Saves the world metadata and optional chunk edit bytes.
    pub fn save_world(meta: &mut WorldMeta, edits_bytes: Option<&[u8]>) -> Result<(), String> {
        let folder = meta.folder_path();
        let _ = fs::create_dir_all(&folder);

        meta.last_played = WorldMeta::current_timestamp_string();
        let toml_str = toml::to_string_pretty(meta)
            .map_err(|e| format!("Failed to serialize world meta: {e}"))?;
        fs::write(meta.meta_file_path(), toml_str)
            .map_err(|e| format!("Failed to write world.toml: {e}"))?;

        if let Some(bytes) = edits_bytes {
            fs::write(meta.edits_file_path(), bytes)
                .map_err(|e| format!("Failed to write edits.dat: {e}"))?;
        }

        Ok(())
    }

    /// Deletes a world directory and all its saved data.
    pub fn delete_world(id: &str) -> Result<(), String> {
        let path = Path::new(WORLDS_DIR).join(id);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|e| format!("Failed to delete world {id}: {e}"))?;
        }
        Ok(())
    }
}
