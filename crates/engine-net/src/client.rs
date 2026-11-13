use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};

use glam::Vec3;

use crate::protocol::{Packet, RemotePlayer, PROTOCOL_VERSION};

#[derive(Debug, Clone)]
pub enum ClientEvent {
    Connected {
        my_id: u32,
        seed: u32,
        time_of_day: f32,
        edits: Vec<([i32; 3], u16)>,
    },
    BlockChange {
        pos: [i32; 3],
        block_id: u16,
    },
    ChatMessage {
        sender_id: u32,
        sender_name: String,
        text: String,
    },
    PlayerJoined {
        id: u32,
        name: String,
    },
    PlayerLeft {
        id: u32,
    },
    HitPlayer {
        attacker_id: u32,
        target_id: u32,
        damage: f32,
        knockback: [f32; 3],
    },
    PlayerHealth {
        player_id: u32,
        health: f32,
    },
    Disconnected,
}

pub struct GameClient {
    stream: TcpStream,
    pub server_addr: SocketAddr,
    pub my_id: u32,
    pub is_connected: bool,
    pub remote_players: HashMap<u32, RemotePlayer>,
    read_buf: Vec<u8>,
}

impl GameClient {
    pub fn connect(server_addr: SocketAddr, player_name: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect_timeout(&server_addr, std::time::Duration::from_secs(12))?;
        stream.set_nonblocking(true)?;
        let _ = stream.set_nodelay(true);

        let mut client = Self {
            stream,
            server_addr,
            my_id: 0,
            is_connected: false,
            remote_players: HashMap::new(),
            read_buf: Vec::with_capacity(8192),
        };

        // Send Join handshake packet
        let join = Packet::Join {
            protocol: PROTOCOL_VERSION,
            name: player_name.to_string(),
        };
        client.send_packet(&join)?;

        Ok(client)
    }

    pub fn send_packet(&mut self, packet: &Packet) -> std::io::Result<()> {
        let bytes = packet.encode();
        self.stream.write_all(&bytes)
    }

    pub fn send_player_state(
        &mut self,
        pos: Vec3,
        yaw: f32,
        pitch: f32,
        walk_phase: f32,
        is_moving: bool,
        arm_swing: f32,
        held_block: u16,
    ) {
        if !self.is_connected {
            return;
        }
        let packet = Packet::PlayerState {
            id: self.my_id,
            pos: pos.to_array(),
            yaw,
            pitch,
            walk_phase,
            is_moving,
            arm_swing,
            held_block,
        };
        let _ = self.send_packet(&packet);
    }

    pub fn send_block_change(&mut self, pos: [i32; 3], block_id: u16) {
        if !self.is_connected {
            return;
        }
        let packet = Packet::BlockChange { pos, block_id };
        let _ = self.send_packet(&packet);
    }

    pub fn send_chat(&mut self, text: &str) {
        if !self.is_connected {
            return;
        }
        let packet = Packet::ChatMessage {
            sender_id: self.my_id,
            sender_name: String::new(),
            text: text.to_string(),
        };
        let _ = self.send_packet(&packet);
    }

    pub fn update_remote_players(&mut self, dt: f32) {
        for player in self.remote_players.values_mut() {
            player.interpolate(dt);
        }
    }

    pub fn poll_events(&mut self) -> Vec<ClientEvent> {
        let mut events = Vec::new();
        let mut temp_buf = [0u8; 4096];

        loop {
            match self.stream.read(&mut temp_buf) {
                Ok(0) => {
                    self.is_connected = false;
                    events.push(ClientEvent::Disconnected);
                    return events;
                }
                Ok(n) => {
                    self.read_buf.extend_from_slice(&temp_buf[..n]);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(_) => {
                    self.is_connected = false;
                    events.push(ClientEvent::Disconnected);
                    return events;
                }
            }
        }

        while let Some((packet, consumed)) = Packet::decode(&self.read_buf) {
            self.read_buf.drain(0..consumed);

            match packet {
                Packet::Welcome {
                    assigned_id,
                    seed,
                    time_of_day,
                    edits,
                } => {
                    self.my_id = assigned_id;
                    self.is_connected = true;
                    events.push(ClientEvent::Connected {
                        my_id: assigned_id,
                        seed,
                        time_of_day,
                        edits,
                    });
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
                    if id != self.my_id {
                        let target_pos = Vec3::from_array(pos);
                        let player = self
                            .remote_players
                            .entry(id)
                            .or_insert_with(|| RemotePlayer::new(id, format!("Player_{id}"), target_pos));
                        player.target_pos = target_pos;
                        player.target_yaw = yaw;
                        player.target_pitch = pitch;
                        player.walk_phase = walk_phase;
                        player.is_moving = is_moving;
                        player.arm_swing = arm_swing;
                        player.held_block = held_block;
                    }
                }
                Packet::BlockChange { pos, block_id } => {
                    events.push(ClientEvent::BlockChange { pos, block_id });
                }
                Packet::ChatMessage {
                    sender_id,
                    sender_name,
                    text,
                } => {
                    events.push(ClientEvent::ChatMessage {
                        sender_id,
                        sender_name,
                        text,
                    });
                }
                Packet::PlayerJoined { id, name } => {
                    if id != self.my_id {
                        self.remote_players
                            .entry(id)
                            .or_insert_with(|| RemotePlayer::new(id, name.clone(), Vec3::ZERO));
                        events.push(ClientEvent::PlayerJoined { id, name });
                    }
                }
                Packet::PlayerLeft { id } => {
                    self.remote_players.remove(&id);
                    events.push(ClientEvent::PlayerLeft { id });
                }
                Packet::HitPlayer {
                    attacker_id,
                    target_id,
                    damage,
                    knockback,
                } => {
                    events.push(ClientEvent::HitPlayer {
                        attacker_id,
                        target_id,
                        damage,
                        knockback,
                    });
                }
                Packet::PlayerHealth { player_id, health } => {
                    if let Some(player) = self.remote_players.get_mut(&player_id) {
                        player.health = health;
                    }
                    events.push(ClientEvent::PlayerHealth { player_id, health });
                }
                _ => {}
            }
        }

        events
    }
}
