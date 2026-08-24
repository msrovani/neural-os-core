// Globals for NIC drivers. Initialized by hermes::net during boot.
use spin::Mutex;

pub struct NicConfig {
    pub mac: [u8; 6],
    pub ip: [u8; 4],
}
const fn nic_config_new() -> NicConfig { NicConfig { mac: [0; 6], ip: [0; 4] } }

/// Wall-clock pause via TSC (calibrated sleep_us if available, else rdtsc fixed).
#[inline]
pub fn wall_pause_us(us: u64) {
    crate::tsc::sleep_us(us);
}

// SESSION_234: statics em `.data` (NÃO .bss) — o bump heap/talc sobrescrevia
// o .bss entre o setter (T+9) e o p2p_tick (T+167): set_nic_config gravava o
// MAC, p2p_tick lia zeros → gate ready falhava → zero TX heartbeats. Mesmo
// padrão do SESSION_233 (GLOBAL_ALLOCATOR/PHYS_MEM_OFFSET/TOTAL_RAM_MB).
#[link_section = ".data"]
pub static RTL8139: Mutex<Option<crate::rtl8139::Rtl8139Driver>> = Mutex::new(None);
#[link_section = ".data"]
pub static E1000: Mutex<Option<crate::e1000::E1000Driver>> = Mutex::new(None);
#[link_section = ".data"]
pub static I225: Mutex<Option<crate::i225::I225Driver>> = Mutex::new(None);
#[link_section = ".data"]
pub static VIRTIO_DEV: Mutex<Option<crate::virtio_net::VirtIoDevice>> = Mutex::new(None);
#[link_section = ".data"]
pub static NET_CONFIG: Mutex<NicConfig> = Mutex::new(nic_config_new());

/// Sincroniza MAC/IP para o NET_CONFIG do k_nano. Chamado pelo bin quando a
/// rede é configurada (driver init + static IP/DHCP). O transporte P2P (R0)
/// lê daqui para montar os frames broadcast.
pub fn set_nic_config(mac: [u8; 6], ip: [u8; 4]) {
    let mut cfg = NET_CONFIG.lock();
    cfg.mac = mac;
    cfg.ip = ip;
}
