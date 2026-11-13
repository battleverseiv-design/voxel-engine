pub mod client;
pub mod discovery;
pub mod host;
pub mod protocol;

pub use client::{ClientEvent, GameClient};
pub use discovery::{get_available_local_ips, DiscoveredServer, DiscoveryBeacon, DiscoveryScanner};
pub use host::{HostEvent, ServerHost};
pub use protocol::{Packet, RemotePlayer, DEFAULT_PORT, DISCOVERY_PORT, PROTOCOL_VERSION};
