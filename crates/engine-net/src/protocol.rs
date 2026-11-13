use engine_core::SandboxOverrides;
use glam::Vec3;

pub const PROTOCOL_VERSION: u32 = 2;
pub const DEFAULT_PORT: u16 = 25565;
pub const DISCOVERY_PORT: u16 = 25566;

/// State of a remote player connected to the game.
#[derive(Debug, Clone)]
pub struct RemotePlayer {
    pub id: u32,
    pub name: String,
    pub current_pos: Vec3,
    pub target_pos: Vec3,
    pub current_yaw: f32,
    pub target_yaw: f32,
    pub current_pitch: f32,
    pub target_pitch: f32,
    pub walk_phase: f32,
    pub is_moving: bool,
    pub arm_swing: f32,
    pub held_block: u16,
    pub health: f32,
    pub hurt_timer: f32,
}

impl RemotePlayer {
    pub fn new(id: u32, name: String, pos: Vec3) -> Self {
        Self {
            id,
            name,
            current_pos: pos,
            target_pos: pos,
            current_yaw: 0.0,
            target_yaw: 0.0,
            current_pitch: 0.0,
            target_pitch: 0.0,
            walk_phase: 0.0,
            is_moving: false,
            arm_swing: 0.0,
            held_block: 0,
            health: 100.0,
            hurt_timer: 0.0,
        }
    }

    /// Smoothly interpolates position and rotation towards target received from network.
    pub fn interpolate(&mut self, dt: f32) {
        let alpha = (dt * 15.0).clamp(0.0, 1.0);
        self.current_pos = self.current_pos.lerp(self.target_pos, alpha);
        self.current_yaw += (self.target_yaw - self.current_yaw) * alpha;
        self.current_pitch += (self.target_pitch - self.current_pitch) * alpha;
        if self.hurt_timer > 0.0 {
            self.hurt_timer = (self.hurt_timer - dt).max(0.0);
        }
    }
}

/// Strongly-typed network packets for synchronization.
#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    /// Client joins with display name and optional password
    Join {
        protocol: u32,
        name: String,
        password: Option<String>,
    },
    /// Server accepts client and provides world state and delta edits
    Welcome {
        assigned_id: u32,
        seed: u32,
        time_of_day: f32,
        edits: Vec<([i32; 3], u16)>,
    },
    /// Player spatial and animation state
    PlayerState {
        id: u32,
        pos: [f32; 3],
        yaw: f32,
        pitch: f32,
        walk_phase: f32,
        is_moving: bool,
        arm_swing: f32,
        held_block: u16,
    },
    /// Voxel modification in the world
    BlockChange {
        pos: [i32; 3],
        block_id: u16,
    },
    /// In-game text chat message
    ChatMessage {
        sender_id: u32,
        sender_name: String,
        text: String,
    },
    /// Notification when another player joins
    PlayerJoined {
        id: u32,
        name: String,
    },
    /// Notification when a player disconnects
    PlayerLeft {
        id: u32,
    },
    /// Damage / attack event in PvP
    HitPlayer {
        attacker_id: u32,
        target_id: u32,
        damage: f32,
        knockback: [f32; 3],
    },
    /// Player health sync
    PlayerHealth {
        player_id: u32,
        health: f32,
    },
    /// Host handshake sent to client upon connecting with server profile rules & sandbox
    HostHandshake {
        server_name: String,
        experience_id: String,
        sandbox: SandboxOverrides,
        experience_manifest: String,
    },
    /// Rejection or disconnection with a descriptive message
    DisconnectReason {
        reason: String,
    },
    /// Admin action: Kick player with reason
    Kick {
        reason: String,
    },
    /// Admin action: Teleport player to destination coordinates
    Teleport {
        pos: [f32; 3],
    },
    /// Host updates sandbox physics/rules on the fly
    RulesUpdate {
        sandbox: SandboxOverrides,
    },
}

#[inline]
fn write_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    buf.extend_from_slice(bytes);
}

#[inline]
fn read_str(slice: &[u8], cur: &mut usize) -> Option<String> {
    if *cur + 2 > slice.len() {
        return None;
    }
    let len = u16::from_le_bytes(slice[*cur..*cur + 2].try_into().ok()?) as usize;
    *cur += 2;
    if *cur + len > slice.len() {
        return None;
    }
    let s = String::from_utf8_lossy(&slice[*cur..*cur + len]).to_string();
    *cur += len;
    Some(s)
}

#[inline]
fn write_sandbox(buf: &mut Vec<u8>, s: &SandboxOverrides) {
    buf.extend_from_slice(&s.population_multiplier.to_le_bytes());
    write_str(buf, &s.spawn_mode);
    buf.extend_from_slice(&s.gravity_multiplier.to_le_bytes());
    buf.extend_from_slice(&s.voxel_scale.to_le_bytes());
    buf.push(if s.pvp_enabled { 1 } else { 0 });
    buf.push(if s.structural_integrity { 1 } else { 0 });
}

#[inline]
fn read_sandbox(slice: &[u8], cur: &mut usize) -> Option<SandboxOverrides> {
    if *cur + 4 > slice.len() {
        return None;
    }
    let population_multiplier = f32::from_le_bytes(slice[*cur..*cur + 4].try_into().ok()?);
    *cur += 4;
    let spawn_mode = read_str(slice, cur)?;
    if *cur + 4 + 4 + 1 + 1 > slice.len() {
        return None;
    }
    let gravity_multiplier = f32::from_le_bytes(slice[*cur..*cur + 4].try_into().ok()?);
    *cur += 4;
    let voxel_scale = f32::from_le_bytes(slice[*cur..*cur + 4].try_into().ok()?);
    *cur += 4;
    let pvp_enabled = slice[*cur] != 0;
    *cur += 1;
    let structural_integrity = slice[*cur] != 0;
    *cur += 1;

    Some(SandboxOverrides {
        population_multiplier,
        spawn_mode,
        gravity_multiplier,
        voxel_scale,
        pvp_enabled,
        structural_integrity,
    })
}

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Reserve 4 bytes for total length prefix
        buf.extend_from_slice(&[0, 0, 0, 0]);

        match self {
            Packet::Join { protocol, name, password } => {
                buf.push(1);
                buf.extend_from_slice(&protocol.to_le_bytes());
                write_str(&mut buf, name);
                if let Some(pw) = password {
                    buf.push(1);
                    write_str(&mut buf, pw);
                } else {
                    buf.push(0);
                }
            }
            Packet::Welcome {
                assigned_id,
                seed,
                time_of_day,
                edits,
            } => {
                buf.push(2);
                buf.extend_from_slice(&assigned_id.to_le_bytes());
                buf.extend_from_slice(&seed.to_le_bytes());
                buf.extend_from_slice(&time_of_day.to_le_bytes());
                buf.extend_from_slice(&(edits.len() as u32).to_le_bytes());
                for (pos, block_id) in edits {
                    buf.extend_from_slice(&pos[0].to_le_bytes());
                    buf.extend_from_slice(&pos[1].to_le_bytes());
                    buf.extend_from_slice(&pos[2].to_le_bytes());
                    buf.extend_from_slice(&block_id.to_le_bytes());
                }
            }
            Packet::PlayerState {
                id,
                pos,
                yaw,
                pitch,
                walk_phase,
                is_moving,
                arm_swing,
                held_block,
            } => {
                buf.push(3);
                buf.extend_from_slice(&id.to_le_bytes());
                buf.extend_from_slice(&pos[0].to_le_bytes());
                buf.extend_from_slice(&pos[1].to_le_bytes());
                buf.extend_from_slice(&pos[2].to_le_bytes());
                buf.extend_from_slice(&yaw.to_le_bytes());
                buf.extend_from_slice(&pitch.to_le_bytes());
                buf.extend_from_slice(&walk_phase.to_le_bytes());
                buf.push(if *is_moving { 1 } else { 0 });
                buf.extend_from_slice(&arm_swing.to_le_bytes());
                buf.extend_from_slice(&held_block.to_le_bytes());
            }
            Packet::BlockChange { pos, block_id } => {
                buf.push(4);
                buf.extend_from_slice(&pos[0].to_le_bytes());
                buf.extend_from_slice(&pos[1].to_le_bytes());
                buf.extend_from_slice(&pos[2].to_le_bytes());
                buf.extend_from_slice(&block_id.to_le_bytes());
            }
            Packet::ChatMessage {
                sender_id,
                sender_name,
                text,
            } => {
                buf.push(5);
                buf.extend_from_slice(&sender_id.to_le_bytes());
                write_str(&mut buf, sender_name);
                write_str(&mut buf, text);
            }
            Packet::PlayerJoined { id, name } => {
                buf.push(6);
                buf.extend_from_slice(&id.to_le_bytes());
                write_str(&mut buf, name);
            }
            Packet::PlayerLeft { id } => {
                buf.push(7);
                buf.extend_from_slice(&id.to_le_bytes());
            }
            Packet::HitPlayer {
                attacker_id,
                target_id,
                damage,
                knockback,
            } => {
                buf.push(8);
                buf.extend_from_slice(&attacker_id.to_le_bytes());
                buf.extend_from_slice(&target_id.to_le_bytes());
                buf.extend_from_slice(&damage.to_le_bytes());
                buf.extend_from_slice(&knockback[0].to_le_bytes());
                buf.extend_from_slice(&knockback[1].to_le_bytes());
                buf.extend_from_slice(&knockback[2].to_le_bytes());
            }
            Packet::PlayerHealth { player_id, health } => {
                buf.push(9);
                buf.extend_from_slice(&player_id.to_le_bytes());
                buf.extend_from_slice(&health.to_le_bytes());
            }
            Packet::HostHandshake {
                server_name,
                experience_id,
                sandbox,
                experience_manifest,
            } => {
                buf.push(10);
                write_str(&mut buf, server_name);
                write_str(&mut buf, experience_id);
                write_sandbox(&mut buf, sandbox);
                write_str(&mut buf, experience_manifest);
            }
            Packet::DisconnectReason { reason } => {
                buf.push(11);
                write_str(&mut buf, reason);
            }
            Packet::Kick { reason } => {
                buf.push(12);
                write_str(&mut buf, reason);
            }
            Packet::Teleport { pos } => {
                buf.push(13);
                buf.extend_from_slice(&pos[0].to_le_bytes());
                buf.extend_from_slice(&pos[1].to_le_bytes());
                buf.extend_from_slice(&pos[2].to_le_bytes());
            }
            Packet::RulesUpdate { sandbox } => {
                buf.push(14);
                write_sandbox(&mut buf, sandbox);
            }
        }

        // Fill length prefix (payload length = total - 4)
        let payload_len = (buf.len() - 4) as u32;
        buf[0..4].copy_from_slice(&payload_len.to_le_bytes());
        buf
    }

    /// Decodes a packet from buffer. Returns `Some((Packet, bytes_consumed))` or `None` if incomplete.
    pub fn decode(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() < 4 {
            return None;
        }
        let payload_len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let total_len = 4 + payload_len;
        if buf.len() < total_len {
            return None;
        }

        let slice = &buf[4..total_len];
        if slice.is_empty() {
            return None;
        }
        let tag = slice[0];
        let mut cur = 1;

        let packet = match tag {
            1 => {
                if cur + 4 > slice.len() {
                    return None;
                }
                let protocol = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let name = read_str(slice, &mut cur)?;
                let password = if cur < slice.len() && slice[cur] != 0 {
                    cur += 1;
                    read_str(slice, &mut cur)
                } else {
                    None
                };
                Packet::Join { protocol, name, password }
            }
            2 => {
                if cur + 4 + 4 + 4 + 4 > slice.len() {
                    return None;
                }
                let assigned_id = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let seed = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let time_of_day = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let edits_count = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?) as usize;
                cur += 4;
                let mut edits = Vec::with_capacity(edits_count.min(10000));
                for _ in 0..edits_count {
                    if cur + 12 + 2 > slice.len() {
                        break;
                    }
                    let x = i32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                    cur += 4;
                    let y = i32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                    cur += 4;
                    let z = i32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                    cur += 4;
                    let block_id = u16::from_le_bytes(slice[cur..cur + 2].try_into().ok()?);
                    cur += 2;
                    edits.push(([x, y, z], block_id));
                }
                Packet::Welcome {
                    assigned_id,
                    seed,
                    time_of_day,
                    edits,
                }
            }
            3 => {
                if cur + 4 + 12 + 8 + 4 + 1 + 4 + 2 > slice.len() {
                    return None;
                }
                let id = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let x = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let y = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let z = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let yaw = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let pitch = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let walk_phase = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let is_moving = slice[cur] != 0;
                cur += 1;
                let arm_swing = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let held_block = u16::from_le_bytes(slice[cur..cur + 2].try_into().ok()?);
                Packet::PlayerState {
                    id,
                    pos: [x, y, z],
                    yaw,
                    pitch,
                    walk_phase,
                    is_moving,
                    arm_swing,
                    held_block,
                }
            }
            4 => {
                if cur + 12 + 2 > slice.len() {
                    return None;
                }
                let x = i32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let y = i32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let z = i32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let block_id = u16::from_le_bytes(slice[cur..cur + 2].try_into().ok()?);
                Packet::BlockChange {
                    pos: [x, y, z],
                    block_id,
                }
            }
            5 => {
                if cur + 4 > slice.len() {
                    return None;
                }
                let sender_id = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let sender_name = read_str(slice, &mut cur)?;
                let text = read_str(slice, &mut cur)?;
                Packet::ChatMessage {
                    sender_id,
                    sender_name,
                    text,
                }
            }
            6 => {
                if cur + 4 > slice.len() {
                    return None;
                }
                let id = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let name = read_str(slice, &mut cur)?;
                Packet::PlayerJoined { id, name }
            }
            7 => {
                if cur + 4 > slice.len() {
                    return None;
                }
                let id = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                Packet::PlayerLeft { id }
            }
            8 => {
                if cur + 4 + 4 + 4 + 12 > slice.len() {
                    return None;
                }
                let attacker_id = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let target_id = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let damage = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let kx = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let ky = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let kz = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                Packet::HitPlayer {
                    attacker_id,
                    target_id,
                    damage,
                    knockback: [kx, ky, kz],
                }
            }
            9 => {
                if cur + 4 + 4 > slice.len() {
                    return None;
                }
                let player_id = u32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let health = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                Packet::PlayerHealth { player_id, health }
            }
            10 => {
                let server_name = read_str(slice, &mut cur)?;
                let experience_id = read_str(slice, &mut cur)?;
                let sandbox = read_sandbox(slice, &mut cur)?;
                let experience_manifest = read_str(slice, &mut cur)?;
                Packet::HostHandshake {
                    server_name,
                    experience_id,
                    sandbox,
                    experience_manifest,
                }
            }
            11 => {
                let reason = read_str(slice, &mut cur)?;
                Packet::DisconnectReason { reason }
            }
            12 => {
                let reason = read_str(slice, &mut cur)?;
                Packet::Kick { reason }
            }
            13 => {
                if cur + 12 > slice.len() {
                    return None;
                }
                let px = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let py = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                cur += 4;
                let pz = f32::from_le_bytes(slice[cur..cur + 4].try_into().ok()?);
                Packet::Teleport { pos: [px, py, pz] }
            }
            14 => {
                let sandbox = read_sandbox(slice, &mut cur)?;
                Packet::RulesUpdate { sandbox }
            }
            _ => return None,
        };

        Some((packet, total_len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_roundtrip() {
        let p = Packet::Join {
            protocol: PROTOCOL_VERSION,
            name: "Steve".to_string(),
            password: Some("secret123".to_string()),
        };
        let encoded = p.encode();
        let (decoded, consumed) = Packet::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, p);
    }

    #[test]
    fn test_host_handshake_roundtrip() {
        let p = Packet::HostHandshake {
            server_name: "Mega Server".to_string(),
            experience_id: "survival".to_string(),
            sandbox: SandboxOverrides {
                population_multiplier: 1.5,
                spawn_mode: "Migration".to_string(),
                gravity_multiplier: 1.2,
                voxel_scale: 1.0,
                pvp_enabled: false,
                structural_integrity: true,
            },
            experience_manifest: "manifest_v1".to_string(),
        };
        let encoded = p.encode();
        let (decoded, consumed) = Packet::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, p);
    }

    #[test]
    fn test_admin_packets_roundtrip() {
        let kick = Packet::Kick { reason: "Bad ping".into() };
        let (d_kick, _) = Packet::decode(&kick.encode()).unwrap();
        assert_eq!(d_kick, kick);

        let tp = Packet::Teleport { pos: [12.0, 64.0, -50.0] };
        let (d_tp, _) = Packet::decode(&tp.encode()).unwrap();
        assert_eq!(d_tp, tp);

        let rules = Packet::RulesUpdate {
            sandbox: SandboxOverrides::default(),
        };
        let (d_rules, _) = Packet::decode(&rules.encode()).unwrap();
        assert_eq!(d_rules, rules);
    }
}
