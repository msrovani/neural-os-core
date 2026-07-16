// Globals for NIC drivers. Initialized by hermes::net during boot.
use spin::Mutex;

pub struct NicConfig {
    pub mac: [u8; 6],
}
const fn nic_config_new() -> NicConfig { NicConfig { mac: [0; 6] } }

pub static RTL8139: Mutex<Option<crate::rtl8139::Rtl8139Driver>> = Mutex::new(None);
pub static E1000: Mutex<Option<crate::e1000::E1000Driver>> = Mutex::new(None);
pub static VIRTIO_DEV: Mutex<Option<crate::virtio_net::VirtIoDevice>> = Mutex::new(None);
pub static NET_CONFIG: Mutex<NicConfig> = Mutex::new(nic_config_new());
