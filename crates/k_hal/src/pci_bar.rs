//! Decode PCI BAR (32/64-bit mem + I/O) — evita OR errado de bar1 em BAR 32-bit.

/// Extrai endereço físico de um par BAR low/high.
pub fn decode_bar(bar_lo: u32, bar_hi: u32) -> u64 {
    if bar_lo & 1 != 0 {
        // I/O space
        return (bar_lo as u64) & !0x3;
    }
    let is_64 = (bar_lo & 0x6) == 0x4;
    let addr = if is_64 {
        ((bar_lo as u64) & !0xF) | ((bar_hi as u64) << 32)
    } else {
        // 32-bit memory: NÃO misturar com o próximo BAR
        (bar_lo as u64) & !0xF
    };
    // Mapper x86_64: rejeita phys com bits 48+ (não-canônico após +pmoff)
    if addr == 0 || addr == !0 || (addr >> 48) != 0 {
        return 0;
    }
    addr
}
