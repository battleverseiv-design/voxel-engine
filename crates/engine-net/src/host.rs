use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use engine_core::{SandboxOverrides, ServerHostProfile};
use glam::Vec3;

use crate::protocol::{Packet, PROTOCOL_VERSION};

pub struct ConnectedClient {
    pub id: u32,
    pub name: String,
    pub stream: TcpStream,
    pub addr: SocketAddr,
    pub read_buf: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum HostEvent {
    ClientJoined {
        id: u32,
        name: String,
        addr: SocketAddr,
    },
    ClientLeft {
        id: u32,
        name: String,
    },
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
    BlockChange {
        pos: [i32; 3],
        block_id: u16,
    },
    ChatMessage {
        sender_id: u32,
        sender_name: String,
        text: String,
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
}

pub struct ServerHost {
    listener: TcpListener,
    pub profile: ServerHostProfile,
    pub time_of_day: f32,
    pub edits: Vec<([i32; 3], u16)>,
    clients: Vec<ConnectedClient>,
    next_client_id: u32,
}

impl ServerHost {
    pub fn start(
        profile: ServerHostProfile,
        time_of_day: f32,
        initial_edits: Vec<([i32; 3], u16)>,
    ) -> std::io::Result<Self> {
        let port = profile.port;
        let listener = TcpListener::bind(format!("0.0.0.0:{port}"))?;
        listener.set_nonblocking(true)?;

        Ok(Self {
            listener,
            profile,
            time_of_day,
            edits: initial_edits,
            clients: Vec::new(),
            next_client_id: 1, // 0 is reserved for local host player
        })
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    pub fn clients(&self) -> &[ConnectedClient] {
        &self.clients
    }

    pub fn get_client(&self, id: u32) -> Option<&ConnectedClient> {
        self.clients.iter().find(|c| c.id == id)
    }

    pub fn port(&self) -> u16 {
        self.profile.port
    }

    pub fn broadcast(&mut self, packet: &Packet) {
        let bytes = packet.encode();
        for client in &mut self.clients {
            let _ = client.stream.write_all(&bytes);
        }
    }

    pub fn broadcast_except(&mut self, except_id: u32, packet: &Packet) {
        let bytes = packet.encode();
        for client in &mut self.clients {
            if client.id != except_id {
                let _ = client.stream.write_all(&bytes);
            }
        }
    }

    /// Admin function: kicks a connected client with an explanation reason.
    pub fn kick_client(&mut self, id: u32, reason: &str) -> Option<HostEvent> {
        if let Some(pos) = self.clients.iter().position(|c| c.id == id) {
            let mut client = self.clients.remove(pos);
            let kick_pkt = Packet::Kick {
                reason: reason.to_string(),
            };
            let _ = client.stream.write_all(&kick_pkt.encode());
            let _ = client.stream.flush();

            let left_event = HostEvent::ClientLeft {
                id: client.id,
                name: client.name.clone(),
            };
            self.broadcast(&Packet::PlayerLeft { id });
            Some(left_event)
        } else {
            None
        }
    }

    /// Admin function: teleports a connected client directly to the specified coordinates.
    pub fn teleport_client(&mut self, id: u32, pos: Vec3) {
        if let Some(client) = self.clients.iter_mut().find(|c| c.id == id) {
            let tp_pkt = Packet::Teleport {
                pos: pos.to_array(),
            };
            let _ = client.stream.write_all(&tp_pkt.encode());
        }
    }

    /// Admin function: updates the sandbox physics and rules on the fly, immediately broadcasting to all clients.
    pub fn update_sandbox_rules(&mut self, sandbox: SandboxOverrides) {
        self.profile.sandbox = sandbox.clone();
        self.broadcast(&Packet::RulesUpdate { sandbox });
    }

    pub fn poll_events(&mut self) -> Vec<HostEvent> {
        let mut events = Vec::new();

        // 1. Accept new incoming clients
        while let Ok((stream, addr)) = self.listener.accept() {
            let _ = stream.set_nonblocking(true);
            let _ = stream.set_nodelay(true);
            let id = self.next_client_id;
            self.next_client_id += 1;

            self.clients.push(ConnectedClient {
                id,
                name: format!("Player_{id}"),
                stream,
                addr,
                read_buf: Vec::with_capacity(4096),
            });
        }

        // 2. Read incoming data from existing clients
        let mut disconnected_ids = Vec::new();
        let mut temp_buf = [0u8; 4096];
        let total_clients = self.clients.len();

        for i in 0..total_clients {
            let client = &mut self.clients[i];
            loop {
                match client.stream.read(&mut temp_buf) {
                    Ok(0) => {
                        // Connection closed
                        disconnected_ids.push(client.id);
                        break;
                    }
                    Ok(n) => {
                        client.read_buf.extend_from_slice(&temp_buf[..n]);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(_) => {
                        disconnected_ids.push(client.id);
                        break;
                    }
                }
            }

            // Decode all complete packets from this client's buffer
            while let Some((packet, consumed)) = Packet::decode(&client.read_buf) {
                client.read_buf.drain(0..consumed);

                match packet {
                    Packet::Join { protocol, name, password } => {
                        if protocol != PROTOCOL_VERSION {
                            let disc = Packet::DisconnectReason {
                                reason: format!("Несовпадение версии протокола (Сервер: {PROTOCOL_VERSION}, Клиент: {protocol})"),
                            };
                            let _ = client.stream.write_all(&disc.encode());
                            disconnected_ids.push(client.id);
                            continue;
                        }

                        // Check max players limit
                        if total_clients > self.profile.max_players as usize {
                            let disc = Packet::DisconnectReason {
                                reason: "Сервер переполнен (Server is full)".to_string(),
                            };
                            let _ = client.stream.write_all(&disc.encode());
                            disconnected_ids.push(client.id);
                            continue;
                        }

                        // Check server password
                        if let Some(req_pw) = &self.profile.password {
                            let pw_matches = password.as_deref() == Some(req_pw.as_str());
                            if !pw_matches {
                                let disc = Packet::DisconnectReason {
                                    reason: "Неверный пароль сервера (Incorrect password)".to_string(),
                                };
                                let _ = client.stream.write_all(&disc.encode());
                                disconnected_ids.push(client.id);
                                continue;
                            }
                        }

                        client.name = name.clone();

                        // 1. Send HostHandshake with server profile rules & sandbox
                        let handshake = Packet::HostHandshake {
                            server_name: self.profile.server_name.clone(),
                            experience_id: self.profile.experience_id.clone(),
                            sandbox: self.profile.sandbox.clone(),
                            experience_manifest: "voxel_core_v1".to_string(),
                        };
                        let _ = client.stream.write_all(&handshake.encode());

                        // 2. Send Welcome with assigned id and world state
                        let welcome = Packet::Welcome {
                            assigned_id: client.id,
                            seed: self.profile.world_seed as u32,
                            time_of_day: self.time_of_day,
                            edits: self.edits.clone(),
                        };
                        let _ = client.stream.write_all(&welcome.encode());

                        events.push(HostEvent::ClientJoined {
                            id: client.id,
                            name: client.name.clone(),
                            addr: client.addr,
                        });
                    }
                    Packet::PlayerState {
                        id: _,
                        pos,
                        yaw,
                        pitch,
                        walk_phase,
                        is_moving,
                        arm_swing,
                        held_block,
                    } => {
                        events.push(HostEvent::PlayerState {
                            id: client.id,
                            pos,
                            yaw,
                            pitch,
                            walk_phase,
                            is_moving,
                            arm_swing,
                            held_block,
                        });
                    }
                    Packet::BlockChange { pos, block_id } => {
                        // Record into world delta
                        self.edits.retain(|(p, _)| *p != pos);
                        self.edits.push((pos, block_id));

                        events.push(HostEvent::BlockChange { pos, block_id });
                    }
                    Packet::ChatMessage {
                        sender_id: _,
                        sender_name: _,
                        text,
                    } => {
                        events.push(HostEvent::ChatMessage {
                            sender_id: client.id,
                            sender_name: client.name.clone(),
                            text,
                        });
                    }
                    Packet::HitPlayer {
                        attacker_id,
                        target_id,
                        damage,
                        knockback,
                    } => {
                        // If PvP is disabled in host sandbox, ignore damage packets between players
                        if self.profile.sandbox.pvp_enabled {
                            events.push(HostEvent::HitPlayer {
                                attacker_id,
                                target_id,
                                damage,
                                knockback,
                            });
                        }
                    }
                    Packet::PlayerHealth { player_id, health } => {
                        events.push(HostEvent::PlayerHealth { player_id, health });
                    }
                    _ => {}
                }
            }
        }

        // 3. Remove disconnected clients and broadcast
        for disc_id in disconnected_ids {
            if let Some(pos) = self.clients.iter().position(|c| c.id == disc_id) {
                let removed = self.clients.remove(pos);
                events.push(HostEvent::ClientLeft {
                    id: removed.id,
                    name: removed.name,
                });
                self.broadcast(&Packet::PlayerLeft { id: disc_id });
            }
        }

        // 4. Relay state and block updates to other clients
        for ev in &events {
            match ev {
                HostEvent::ClientJoined { id, name, .. } => {
                    self.broadcast_except(
                        *id,
                        &Packet::PlayerJoined {
                            id: *id,
                            name: name.clone(),
                        },
                    );
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
                    self.broadcast_except(
                        *id,
                        &Packet::PlayerState {
                            id: *id,
                            pos: *pos,
                            yaw: *yaw,
                            pitch: *pitch,
                            walk_phase: *walk_phase,
                            is_moving: *is_moving,
                            arm_swing: *arm_swing,
                            held_block: *held_block,
                        },
                    );
                }
                HostEvent::BlockChange { pos, block_id } => {
                    self.broadcast(&Packet::BlockChange {
                        pos: *pos,
                        block_id: *block_id,
                    });
                }
                HostEvent::ChatMessage {
                    sender_id,
                    sender_name,
                    text,
                } => {
                    self.broadcast(&Packet::ChatMessage {
                        sender_id: *sender_id,
                        sender_name: sender_name.clone(),
                        text: text.clone(),
                    });
                }
                HostEvent::HitPlayer {
                    attacker_id,
                    target_id,
                    damage,
                    knockback,
                } => {
                    self.broadcast(&Packet::HitPlayer {
                        attacker_id: *attacker_id,
                        target_id: *target_id,
                        damage: *damage,
                        knockback: *knockback,
                    });
                }
                HostEvent::PlayerHealth { player_id, health } => {
                    self.broadcast(&Packet::PlayerHealth {
                        player_id: *player_id,
                        health: *health,
                    });
                }
                HostEvent::ClientLeft { .. } => {}
            }
        }

        events
    }
}
