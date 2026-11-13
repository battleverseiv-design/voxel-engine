use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

use crate::protocol::DISCOVERY_PORT;

#[derive(Debug, Clone)]
pub struct DiscoveredServer {
    pub name: String,
    pub addr: SocketAddr,
    pub player_count: u8,
    pub last_seen: Instant,
}

/// UDP Broadcast beacon sender (runs on server host).
pub struct DiscoveryBeacon {
    socket: Option<UdpSocket>,
    port: u16,
    server_name: String,
    last_broadcast: Instant,
}

impl DiscoveryBeacon {
    pub fn new(server_name: String, game_port: u16) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").ok().and_then(|s| {
            s.set_broadcast(true).ok()?;
            s.set_nonblocking(true).ok()?;
            Some(s)
        });

        Self {
            socket,
            port: game_port,
            server_name,
            last_broadcast: Instant::now(),
        }
    }

    pub fn tick(&mut self, player_count: u8) {
        if self.last_broadcast.elapsed().as_secs_f32() >= 1.5 {
            self.last_broadcast = Instant::now();
            if let Some(socket) = &self.socket {
                let msg = format!("VOXEL_BEACON:{}:{}:{}", self.server_name, self.port, player_count);
                let _ = socket.send_to(msg.as_bytes(), format!("255.255.255.255:{DISCOVERY_PORT}"));
            }
        }
    }
}

/// UDP Discovery scanner (runs on client searching for local/Hamachi worlds).
pub struct DiscoveryScanner {
    socket: Option<UdpSocket>,
    pub servers: Vec<DiscoveredServer>,
}

impl DiscoveryScanner {
    pub fn new() -> Self {
        let socket = UdpSocket::bind(format!("0.0.0.0:{DISCOVERY_PORT}")).ok().and_then(|s| {
            s.set_nonblocking(true).ok()?;
            Some(s)
        });

        Self {
            socket,
            servers: Vec::new(),
        }
    }

    pub fn poll(&mut self) {
        let Some(socket) = &self.socket else { return; };
        let mut buf = [0u8; 512];

        while let Ok((n, src)) = socket.recv_from(&mut buf) {
            let text = String::from_utf8_lossy(&buf[..n]);
            if let Some(rest) = text.strip_prefix("VOXEL_BEACON:") {
                let parts: Vec<&str> = rest.split(':').collect();
                if parts.len() >= 3 {
                    let name = parts[0].to_string();
                    let port: u16 = parts[1].parse().unwrap_or(25565);
                    let player_count: u8 = parts[2].parse().unwrap_or(1);
                    let game_addr = SocketAddr::new(src.ip(), port);

                    if let Some(existing) = self.servers.iter_mut().find(|s| s.addr == game_addr) {
                        existing.name = name;
                        existing.player_count = player_count;
                        existing.last_seen = Instant::now();
                    } else {
                        self.servers.push(DiscoveredServer {
                            name,
                            addr: game_addr,
                            player_count,
                            last_seen: Instant::now(),
                        });
                    }
                }
            }
        }

        // Clean up servers not seen for more than 5 seconds
        self.servers.retain(|s| s.last_seen.elapsed().as_secs() < 5);
    }
}

/// Scans local network adapters and returns available IPv4 addresses with friendly labels.
pub fn get_available_local_ips() -> Vec<(String, String)> {
    let mut ips = Vec::new();

    // 1. Primary outbound IP probe
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                let ip_str = local_addr.ip().to_string();
                ips.push((ip_str, "Основной сетевой адаптер (LAN / Wi-Fi)".to_string()));
            }
        }
    }

    // 2. Loopback
    ips.push(("127.0.0.1".to_string(), "Локальный хост (тот же ПК)".to_string()));

    ips
}
