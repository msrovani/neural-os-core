use x86_64::instructions::port::Port;

const COM2: u16 = 0x2F8;
const MTU: usize = 1500;

unsafe fn read_byte() -> u8 {
    while Port::<u8>::new(COM2 + 5).read() & 0x01 == 0 { core::hint::spin_loop(); }
    Port::<u8>::new(COM2).read()
}

unsafe fn write_byte(b: u8) {
    while Port::<u8>::new(COM2 + 5).read() & 0x20 == 0 { core::hint::spin_loop(); }
    Port::<u8>::new(COM2).write(b);
}

/// Verifica se COM2 tem dados para ler (non-blocking).
pub unsafe fn has_data() -> bool {
    Port::<u8>::new(COM2 + 5).read() & 0x01 != 0
}

/// Envia frame pela serial com prefixo de 2 bytes (length big-endian) + dados.
pub unsafe fn send(data: &[u8]) {
    let len = data.len().min(MTU) as u16;
    for &b in &len.to_be_bytes() { write_byte(b); }
    for &b in &data[..len as usize] { write_byte(b); }
}

/// Recebe frame da serial: 2 bytes length (big-endian) + dados.
/// NON-BLOCKING: retorna None se nao houver dados suficientes.
pub unsafe fn recv() -> Option<alloc::vec::Vec<u8>> {
    if !has_data() { return None; }
    let hi = Port::<u8>::new(COM2).read();
    if !has_data() { return None; }
    let lo = Port::<u8>::new(COM2).read();
    let frame_len = u16::from_be_bytes([hi, lo]) as usize;
    if frame_len == 0 || frame_len > MTU { return None; }
    let mut buf = alloc::vec![0u8; frame_len];
    for i in 0..frame_len {
        if !has_data() { return None; }
        buf[i] = Port::<u8>::new(COM2).read();
    }
    Some(buf)
}
