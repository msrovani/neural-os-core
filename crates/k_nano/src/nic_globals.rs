// Globals for NIC drivers. Initialized by hermes::net during boot.
use spin::Mutex;

pub struct NicConfig {
    pub mac: [u8; 6],
    pub ip: [u8; 4],
}
const fn nic_config_new() -> NicConfig { NicConfig { mac: [0; 6], ip: [0; 4] } }

pub static RTL8139: Mutex<Option<crate::rtl8139::Rtl8139Driver>> = Mutex::new(None);
pub static E1000: Mutex<Option<crate::e1000::E1000Driver>> = Mutex::new(None);
pub static VIRTIO_DEV: Mutex<Option<crate::virtio_net::VirtIoDevice>> = Mutex::new(None);
pub static NET_CONFIG: Mutex<NicConfig> = Mutex::new(nic_config_new());

/// Sincroniza MAC/IP para o NET_CONFIG do k_nano. Chamado pelo bin quando a
/// rede é configurada (driver init + static IP/DHCP). O transporte P2P (R0)
/// lê daqui para montar os frames broadcast.
pub fn set_nic_config(mac: [u8; 6], ip: [u8; 4]) {
    let mut cfg = NET_CONFIG.lock();
    cfg.mac = mac;
    cfg.ip = ip;
}
