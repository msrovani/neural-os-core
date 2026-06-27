//! VirtIO-net placeholder.
//! O driver completo requer `virtio-drivers` crate, que depende de `zerocopy-derive`
//! (proc macro incompatível com MinGW toolchain no Windows).
//!
//! Enquanto isso, usamos apenas RTL8139.
//! A struct NetPhy em netstack.rs já unifica NICs — quando VirtIO estiver disponível,
//! basta adicionar send/recv que tenta VIRTIO_DEV depois de RTL8139.

use crate::serial_println;

pub const VIRTIO_NET_VENDOR: u16 = 0x1AF4;
pub const VIRTIO_NET_DEVICE: u16 = 0x1041;

/// Tenta detectar VirtIO-net. Sempre retorna None por enquanto.
pub unsafe fn try_init_virtio_net() -> Option<()> {
    serial_println!("[VIRTIO] VirtIO-net não disponível sem crate. Usando RTL8139.");
    None
}
