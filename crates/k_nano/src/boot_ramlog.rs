//! Boot RAM log — buffer físico espelhando checkpoints (FB/serial sem COM).
//!
//! Phys default `0x1000_0000` (256 MiB) — low mem (16–32 MiB) costuma ser
//! zerada no warm-reset de notebooks (ex.: Note 1050). CRC32 valida sobrevivência.
//!
//! Magic `NEURLOG!` = flush pendente (legado soft-reboot); `NEURDONE` = consumido.
//! Soft-reboot 0xCF9 foi **removido** (bughunt M4) — produto = seal + logwriter-efi.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// 256 MiB — acima da zona que firmware costuma limpar no reset.
pub const BOOT_RAMLOG_PHYS: u64 = 0x1000_0000;
pub const BOOT_RAMLOG_CAP: usize = 256 * 1024;
const HDR_SIZE: usize = 24;

pub const MAGIC_NEED_FLUSH: u64 = u64::from_le_bytes(*b"NEURLOG!");
pub const MAGIC_FLUSHED: u64 = u64::from_le_bytes(*b"NEURDONE");

/// Máximo de recover-reboots consecutivos antes de parquear observável
/// (F1 anti-loop DURÁVEL, OPCODE-0054). Tolera falhas transitórias, para
/// antes do loop infinito de reset.
pub const MAX_RECOVER_REBOOTS: u32 = 3;

/// Flag no header (só o shutdown ordenado arma): o logwriter-efi grava o
/// BOOT.LOG do warm-reset e então desliga via UEFI `ResetSystem(Shutdown)`.
/// S5 direto apagaria a DRAM antes da captura (diagnóstico exp-1).
pub const FLAG_POWEROFF_AFTER_CAPTURE: u32 = 0x504F_4646; // "POFF"

static SKIP_FLUSH_REBOOT: AtomicBool = AtomicBool::new(false);
static INITED: AtomicBool = AtomicBool::new(false);
static LAST_CKPT: AtomicU8 = AtomicU8::new(0);
/// Boot anterior deixou `NEURLOG!` sem o logwriter consumir. Um warm-reset
/// agora repetiria o loop de HW (bughunt M4). Não é o skip do path de flush.
static PRIOR_SEAL_UNFLUSHED: AtomicBool = AtomicBool::new(false);
/// Um recover por sessão. O segundo park fica em hlt, observável no FB.
static RECOVER_RESET_USED: AtomicBool = AtomicBool::new(false);

#[repr(C)]
pub struct BootRamLogHeader {
    pub magic: u64,
    pub len: u32,
    /// CRC32 IEEE dos `len` bytes de payload (ou 0 se vazio).
    pub crc_and_ckpt: u32,
    /// Flags de captura (`FLAG_POWEROFF_AFTER_CAPTURE`). Deve espelhar
    /// `logwriter-efi` (HDR_SIZE = 24).
    pub flags: u32,
    /// Contador DURÁVEL de recover-reboots (offset 20). Sobrevive ao reset, ao
    /// `mark_done` do logwriter (magic@0/crc@12) e ao wipe de `append` quando o
    /// magic ainda é `NEURDONE` (o contador é salvo e restaurado). Espelha
    /// `logwriter-efi`.
    pub recover_count: u32,
}

#[inline]
fn va() -> u64 {
    crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) + BOOT_RAMLOG_PHYS
}

unsafe fn hdr_mut() -> *mut BootRamLogHeader {
    va() as *mut BootRamLogHeader
}

unsafe fn data_ptr() -> *mut u8 {
    (va() + HDR_SIZE as u64) as *mut u8
}

fn data_cap() -> usize {
    BOOT_RAMLOG_CAP.saturating_sub(HDR_SIZE)
}

fn pack_crc_ckpt(crc: u32, ckpt: u8) -> u32 {
    (crc & 0x00FF_FFFF) | ((ckpt as u32) << 24)
}

fn unpack_ckpt(v: u32) -> u8 {
    (v >> 24) as u8
}

/// CRC32 IEEE (poly 0xEDB88320), 24 bits baixos bastam p/ validar.
pub fn crc32_24(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (!(crc & 1)).wrapping_add(1); // 0 or 0xFFFFFFFF
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    (!crc) & 0x00FF_FFFF
}

/// Força a linha do header (magic/len/crc/flags) até o DRAM.
///
/// `sfence` só ORDENA stores — não força write-back de linha suja. Antes de um
/// power-off/troca de sessão o header precisa estar no DRAM: `clflush` da linha
/// + `mfence`. Sem isto o selo evapora na janela de cache (diagnóstico exp-1).
unsafe fn flush_header_line() {
    core::arch::asm!("clflush [{}]", in(reg) va(), options(nostack, preserves_flags));
    core::arch::asm!("mfence", options(nostack, preserves_flags));
}

/// Arma o power-off pós-captura p/ o logwriter-efi (chamado só no shutdown).
pub fn set_poweroff_after_capture() {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return;
    }
    unsafe {
        core::ptr::write_volatile(&mut (*hdr_mut()).flags, FLAG_POWEROFF_AFTER_CAPTURE);
        flush_header_line();
    }
}

/// `true` se o header pede power-off pós-captura (lido pelo logwriter-efi).
pub fn poweroff_after_capture() -> bool {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return false;
    }
    unsafe { core::ptr::read_volatile(&(*hdr_mut()).flags) == FLAG_POWEROFF_AFTER_CAPTURE }
}

pub fn set_last_ckpt(n: u8) {
    LAST_CKPT.store(n, Ordering::Relaxed);
}

pub fn last_ckpt() -> u8 {
    LAST_CKPT.load(Ordering::Relaxed)
}

/// Após map_phys: consome magic legado e **sempre** evita rearmar soft-reboot loop.
///
/// - `NEURDONE` → skip (flush consumido)
/// - `NEURLOG!` → tentativa incompleta do boot anterior (UEFI nunca escreveu DONE) → skip
/// - outro → zera buffer
pub unsafe fn init_from_phys() {
    if INITED.swap(true, Ordering::Relaxed) {
        return;
    }
    let h = &*hdr_mut();
    if h.magic == MAGIC_FLUSHED {
        SKIP_FLUSH_REBOOT.store(true, Ordering::Relaxed);
        let k = unpack_ckpt(h.crc_and_ckpt);
        if k != 0 {
            LAST_CKPT.store(k, Ordering::Relaxed);
        }
        core::ptr::write_volatile(&mut (*hdr_mut()).magic, 0);
        core::ptr::write_volatile(&mut (*hdr_mut()).len, 0);
        crate::slog_nano!(
            "RAMLOG",
            "ok",
            "BOOT.LOG do boot anterior gravado pelo logwriter (K{})",
            k
        );
    } else if h.magic == MAGIC_NEED_FLUSH {
        // Soft-reboot anterior deixou NEURLOG! sem UEFI writer → loop se não skiparmos.
        let k = unpack_ckpt(h.crc_and_ckpt);
        if k != 0 {
            LAST_CKPT.store(k, Ordering::Relaxed);
        }
        SKIP_FLUSH_REBOOT.store(true, Ordering::Relaxed);
        // Selo do boot anterior não foi consumido: o próximo park do BSP
        // não pode chamar warm_reset de novo (loop de reboot no metal).
        PRIOR_SEAL_UNFLUSHED.store(true, Ordering::Relaxed);
        // Marca consumido localmente (não depende de UEFI fantasma).
        core::ptr::write_volatile(&mut (*hdr_mut()).magic, MAGIC_FLUSHED);
        crate::slog_nano!(
            "RAMLOG",
            "warn",
            "NEURLOG! pendente (ckpt K{}) — skip soft-reboot; Runtime segue",
            k
        );
    } else {
        core::ptr::write_bytes(va() as *mut u8, 0, BOOT_RAMLOG_CAP);
    }
}

pub fn skip_flush_reboot() -> bool {
    SKIP_FLUSH_REBOOT.load(Ordering::Relaxed)
}

/// Marca esta sessão para não pedir soft-reboot de novo (continue boot).
pub fn mark_skip_flush_reboot() {
    SKIP_FLUSH_REBOOT.store(true, Ordering::Relaxed);
}

/// Anexa linha (sem alloc). No-op se PHYS_MEM_OFFSET ainda 0.
pub fn append(msg: &str) {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return;
    }
    unsafe {
        let h = &mut *hdr_mut();
        // Selado: snapshot CONGELADO p/ o logwriter — não anexar. Sem isto uma
        // linha pós-selo reescrevia `crc_and_ckpt=pack(0,..)` deixando o magic
        // `NEURLOG!` com CRC=0 → logwriter fail-closed (skip). Era o bug que
        // invalidava o selo do próprio shutdown ordenado (sua linha de evidência
        // e o cascade `power_off` anexavam depois do selo). Ver `seal_core`.
        if h.magic == MAGIC_NEED_FLUSH {
            if SEALED.load(Ordering::Acquire) {
                return; // selo HARD: congela o snapshot
            }
            // Selo SOFT (abort recuperável): nova linha torna o snapshot obsoleto
            // — reabre a coleta (limpa magic/crc); um selo posterior re-arma.
            core::ptr::write_volatile(&mut h.magic, 0);
            core::ptr::write_volatile(&mut h.crc_and_ckpt, pack_crc_ckpt(0, LAST_CKPT.load(Ordering::Relaxed)));
        }
        // Defesa: NEURDONE / magic estranho sem init_from_phys → zera (evita misturar sessões).
        // O wipe é do buffer inteiro, inclusive o offset 20. O recover_count
        // é o único campo que tem de atravessar essa fronteira (FREEBU-0022).
        if h.magic == MAGIC_FLUSHED
            || (h.magic != MAGIC_NEED_FLUSH && h.magic != 0)
        {
            let kept = core::ptr::read_volatile(&h.recover_count);
            core::ptr::write_bytes(va() as *mut u8, 0, BOOT_RAMLOG_CAP);
            core::ptr::write_volatile(&mut (*hdr_mut()).recover_count, kept);
        }
        let mut len = h.len as usize;
        if len >= data_cap() {
            return;
        }
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        let mut line = [0u8; 320];
        let mut pos = 0usize;
        let pfx = b"[T+";
        for &b in pfx {
            line[pos] = b;
            pos += 1;
        }
        let mut t = tick;
        let mut tmp = [0u8; 20];
        let mut ti = 0usize;
        if t == 0 {
            tmp[0] = b'0';
            ti = 1;
        } else {
            while t > 0 && ti < 20 {
                tmp[ti] = b'0' + (t % 10) as u8;
                t /= 10;
                ti += 1;
            }
        }
        while ti > 0 {
            ti -= 1;
            line[pos] = tmp[ti];
            pos += 1;
        }
        line[pos] = b']';
        pos += 1;
        line[pos] = b' ';
        pos += 1;
        for &b in msg.as_bytes() {
            if pos >= line.len() - 1 {
                break;
            }
            line[pos] = b;
            pos += 1;
        }
        line[pos] = b'\n';
        pos += 1;
        let n = pos.min(data_cap() - len);
        core::ptr::copy_nonoverlapping(line.as_ptr(), data_ptr().add(len), n);
        len += n;
        core::ptr::write_volatile(&mut h.len, len as u32);
        let ckpt = LAST_CKPT.load(Ordering::Relaxed);
        // CRC parcial atualizado no flush final; aqui so guarda ckpt.
        core::ptr::write_volatile(&mut h.crc_and_ckpt, pack_crc_ckpt(0, ckpt));
        if h.magic != MAGIC_NEED_FLUSH {
            core::ptr::write_volatile(&mut h.magic, 0);
        }
    }
}

/// Selo já efetuado nesta sessão (idempotência + congela `append`).
static SEALED: AtomicBool = AtomicBool::new(false);

/// Buffer de stack p/ a linha de evidência do selo (sem alloc — IRQ-safe).
struct SealLine {
    buf: [u8; 96],
    pos: usize,
}

impl SealLine {
    fn new() -> Self {
        Self { buf: [0; 96], pos: 0 }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("")
    }
}

impl core::fmt::Write for SealLine {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        let n = b.len().min(self.buf.len().saturating_sub(self.pos));
        self.buf[self.pos..self.pos + n].copy_from_slice(&b[..n]);
        self.pos += n;
        Ok(())
    }
}

/// Core do selo: CRC24 + magic `NEURLOG!` + flush real da linha do header.
///
/// **Sem lock, sem alocação, sem slog** — seguro em contexto de exceção
/// (#PF storm / panic / OOM park). Idempotente: a 2ª chamada é no-op.
/// Retorna `Some((ckpt, len, crc))` apenas no selo efetivo; `None` se
/// `PHYS_MEM_OFFSET==0` ou já selado.
fn seal_core() -> Option<(u8, u32, u32)> {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return None;
    }
    if SEALED.swap(true, Ordering::AcqRel) {
        return None; // já selado nesta sessão
    }
    unsafe {
        let h = &mut *hdr_mut();
        let len = h.len as usize;
        let slice = core::slice::from_raw_parts(data_ptr(), len.min(data_cap()));
        let crc = crc32_24(slice);
        let ckpt = LAST_CKPT.load(Ordering::Relaxed);
        core::ptr::write_volatile(&mut h.crc_and_ckpt, pack_crc_ckpt(crc, ckpt));
        core::ptr::write_volatile(&mut h.magic, MAGIC_NEED_FLUSH);
        flush_header_line();
        Some((ckpt, len as u32, crc))
    }
}

/// Finaliza CRC + magic `NEURLOG!` para o logwriter UEFI no próximo boot.
/// Não reinicia — o caller faz reboot ordenado (`shutdown` / panic).
///
/// Sem isto o stage-0 (`logwriter-efi`) vê magic 0 e não grava BOOT.LOG.
/// Contexto **não-exceção** (shutdown/HITL): a evidência vai a serial/FB.
/// Idempotente. Retorna `true` se o ramlog está selado (ou não há phys).
pub fn seal_for_next_boot() -> bool {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return false;
    }
    if SEALED.load(Ordering::Acquire) {
        return true; // idempotente
    }
    // Linha de evidência DENTRO do buffer antes do selo (append é lock-free);
    // com o snapshot congelado o `slog` abaixo já não a anexaria.
    {
        let ckpt = LAST_CKPT.load(Ordering::Relaxed);
        let len = unsafe { (*hdr_mut()).len };
        let mut b = SealLine::new();
        let _ = core::fmt::write(
            &mut b,
            format_args!("RAMLOG seal_for_next_boot ckpt=K{} len={}", ckpt, len),
        );
        append(b.as_str());
    }
    match seal_core() {
        Some((ckpt, len, crc)) => {
            crate::slog_nano!(
                "RAMLOG",
                "ok",
                "seal_for_next_boot ckpt=K{} len={} crc={:#x} (logwriter)",
                ckpt,
                len,
                crc
            );
            true
        }
        None => SEALED.load(Ordering::Acquire),
    }
}

/// Selo puro p/ contexto de exceção (#PF storm / panic / OOM park) — sem
/// lock, sem alocação, sem slog. Idempotente.
pub fn seal_for_next_boot_quiet() -> bool {
    if SEALED.load(Ordering::Acquire) {
        return true;
    }
    let _ = seal_core();
    SEALED.load(Ordering::Acquire)
}

/// Selo **SOFT** p/ abort RECUPERÁVEL (P6 `fault_abort`): grava CRC+magic
/// para o logwriter capturar o log caso o core morra logo após o fault, mas
/// **não congela** — qualquer `append` posterior reabre a coleta. Sem lock,
/// sem alocação, sem slog (contexto de exceção). Idempotente com o selo hard.
pub fn seal_for_next_boot_soft() -> bool {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return false;
    }
    if SEALED.load(Ordering::Acquire) {
        return true; // já congelado por um selo HARD
    }
    unsafe {
        let h = &mut *hdr_mut();
        let len = h.len as usize;
        let slice = core::slice::from_raw_parts(data_ptr(), len.min(data_cap()));
        let crc = crc32_24(slice);
        let ckpt = LAST_CKPT.load(Ordering::Relaxed);
        core::ptr::write_volatile(&mut h.crc_and_ckpt, pack_crc_ckpt(crc, ckpt));
        core::ptr::write_volatile(&mut h.magic, MAGIC_NEED_FLUSH);
        flush_header_line();
        true
    }
}

/// HITL / comando: sela sob demanda — o próximo reset (mesmo power cycle
/// manual) faz o logwriter-efi gravar o BOOT.LOG sem shutdown ordenado.
///
/// **Wire:** o bin não tem handler de tecla/comando hoje; quando houver
/// (tecla dedicada ou comando shell), chamar `seal_now()` no gatilho.
pub fn seal_now() -> bool {
    seal_for_next_boot()
}

/// Park do BSP vira warm-reset só uma vez, e nunca se o selo anterior
/// ainda está pendente. AP continua em hlt: reset a partir de um AP
/// apagaria o BSP que ainda desenhava a tela.
pub fn recover_reset_decision(is_bsp: bool, prior_seal_unflushed: bool, already_used: bool) -> bool {
    is_bsp && !prior_seal_unflushed && !already_used
}

/// Anti-loop DURÁVEL (missao §6): o reboot ordenado é permitido enquanto o
/// contador durável de recuperações (offset 20 do header, sobrevive ao reset)
/// estiver abaixo de [`MAX_RECOVER_REBOOTS`]. Falha persistente NÃO pode
/// resetar para sempre — ao esgotar, `reboot_ordered` vira park observável
/// (heartbeat), não hlt mudo. Pura: testável sem phys/header.
pub fn recover_budget_exhausted(durable_count: u32) -> bool {
    durable_count >= MAX_RECOVER_REBOOTS
}

pub fn prior_seal_unflushed() -> bool {
    PRIOR_SEAL_UNFLUSHED.load(Ordering::Relaxed)
}

/// Lê o contador DURÁVEL de recover-reboots (offset 20 do header). 0 sem phys.
fn recover_count_load() -> u32 {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return 0;
    }
    unsafe { core::ptr::read_volatile(&(*hdr_mut()).recover_count) }
}

/// Grava o contador DURÁVEL + força a linha do header ao DRAM (clflush+mfence).
fn recover_count_store(v: u32) {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return;
    }
    unsafe {
        core::ptr::write_volatile(&mut (*hdr_mut()).recover_count, v);
        flush_header_line();
    }
}

/// Leitura pública do contador durável (diagnóstico/logwriter).
pub fn recover_count() -> u32 {
    recover_count_load()
}

/// Zera o contador durável — shutdown ordenado = estado limpo (sem loop).
pub fn clear_recover_count() {
    recover_count_store(0);
}

/// Warm-reset (0x64/FE → 0xCF9) — sempre disponível p/ reboot ordenado / panic.
pub unsafe fn warm_reset() -> ! {
    for _ in 0..1000 {
        core::arch::asm!(
            "mov al, 0xFE",
            "out 0x64, al",
            options(nostack, nomem, preserves_flags)
        );
        core::hint::spin_loop();
    }
    core::arch::asm!(
        "mov al, 0x06",
        "mov dx, 0xCF9",
        "out dx, al",
        options(nostack, nomem, preserves_flags)
    );
    loop {
        core::hint::spin_loop();
    }
}

/// Park observável (F1 anti-loop, OPCODE-0054): NUNCA um `loop{hlt}` mudo.
/// Heartbeat de 10s com o motivo via `interrupts::puts` — lock-free porque o
/// spin pode ter sido disparado de contexto de exceção (#PF/OOM) segurando o
/// lock do serial (lição s438: TicketLock não-reentrante + IRQ = deadlock).
/// ponytail: puts lock-free em vez de `SERIAL.lock()` do OOM-HALT — o heartbeat
/// é a única saída observável; travar no lock do serial seria pior que o park.
///
/// P0.3: `pub` — reusado pelo tail de exceção fatal (BSP/AP), pelo panic handler
/// e pelo OOM handler. NUNCA trocar por `loop{hlt}` mudo.
pub fn park_observable(reason: &str) -> ! {
    let mut last = crate::tsc::now_us();
    loop {
        core::hint::spin_loop();
        let now = crate::tsc::now_us();
        if now.wrapping_sub(last) >= 10_000_000 {
            last = now;
            let mut buf = [0u8; 128];
            let mut pos = 0usize;
            for &b in b"[RECOVER-HALT] parked reason=" {
                if pos < buf.len() {
                    buf[pos] = b;
                    pos += 1;
                }
            }
            for &b in reason.as_bytes() {
                if pos >= buf.len() - 2 {
                    break;
                }
                buf[pos] = b;
                pos += 1;
            }
            buf[pos] = b'\n';
            pos += 1;
            crate::interrupts::puts(&buf[..pos]);
        }
    }
}

/// Reboot ORDENADO observável (s439 / F1): anexa `[RECOVER] reason=...`, sela o
/// ramlog p/ o logwriter-efi e faz `warm_reset`. Reusa seal+warm_reset — não
/// reimplementa o reset. Só o BSP deve chamar (um AP reset apagaria o BSP que
/// ainda desenha a tela). Nunca retorna. O `ip`/`cr2` do fault (quando houver)
/// já foram anexados pelo caller via `ramlog_note`.
///
/// Anti-loop em DUAS camadas (OPCODE-0054): (1) `recover_reset_decision`
/// (selo pendente / reset já usado nesta sessão) e (2) contador DURÁVEL
/// `recover_count` no header — um #PF/OOM determinístico não pode resetar
/// para sempre entre boots. Recusa = park observável (heartbeat), não hlt mudo.
pub fn reboot_ordered(reason: &str) -> ! {
    // Decisão ANTES de selar/resetar (OPCODE-0041).
    let allow = recover_reset_decision(
        true,
        prior_seal_unflushed(),
        RECOVER_RESET_USED.load(Ordering::Relaxed),
    ) && !recover_budget_exhausted(recover_count_load());
    if !allow {
        {
            let mut b = SealLine::new();
            let _ = core::fmt::write(
                &mut b,
                format_args!("[RECOVER] reason={} refused anti-loop park", reason),
            );
            append(b.as_str());
        }
        let _ = seal_for_next_boot_quiet();
        park_observable(reason);
    }
    RECOVER_RESET_USED.store(true, Ordering::Relaxed);
    // Contador DURÁVEL: incrementa + flush ANTES do selo (o selo re-flusha a
    // mesma linha de cache). `append` preserva o campo se ainda vir NEURDONE.
    recover_count_store(recover_count_load().saturating_add(1));
    {
        let mut b = SealLine::new();
        let _ = core::fmt::write(&mut b, format_args!("[RECOVER] reason={}", reason));
        append(b.as_str());
    }
    let _ = seal_for_next_boot_quiet();
    #[cfg(target_os = "none")]
    {
        unsafe {
            core::arch::asm!("wbinvd", options(nostack, preserves_flags));
            warm_reset();
        }
    }
    // Host (target não-none): sem porta de reset — park observável.
    #[cfg(not(target_os = "none"))]
    {
        park_observable(reason);
    }
}

/// Soft-reboot BOOT.LOG removido (bughunt M4): produto = seal_for_next_boot +
/// orderly reboot + logwriter-efi. API antiga era stub `-> !` com spin infinito.
pub fn maybe_flush_reboot(reason: &str) {
    let _ = reason;
    mark_skip_flush_reboot();
    append("maybe_flush_reboot: soft-reboot removed — seal+logwriter path");
}

/// Ponytail: dump não-bloqueante do ramlog phys no FB/serial (K22/K137 hang).
/// Mantém em RAM quando pendrive não pronto; FB mostra foto para usuário.
pub fn dump() {
    for_each_line(40, |line| {
        crate::slog_nano!("RAMLOG", "dump", "{}", line);
    });
}

/// Emite linhas USB/hub/MSC do ramlog (foto no Alienware sem COM1).
/// Varre **todo** o buffer (não só as 1ªs 200 linhas — F3 false "probe nao chegou").
pub fn dump_usb_hint(mut emit: impl FnMut(&str)) {
    let mut n = 0usize;
    for_each_line(usize::MAX, |line| {
        if n >= 32 {
            return;
        }
        let l = line;
        let lower_ok = l.contains("USB")
            || l.contains("usb")
            || l.contains("hub")
            || l.contains("MSC")
            || l.contains("msc")
            || l.contains("xhci")
            || l.contains("XHCI")
            || l.contains("BOOT.LOG")
            || l.contains("CCS")
            || l.contains("PORTSC");
        if lower_ok {
            emit(l);
            n += 1;
        }
    });
    if n == 0 {
        emit("USB: ramlog sem linhas hub/MSC (slog pode ser serial-only)");
    }
}

fn for_each_line(max: usize, mut emit: impl FnMut(&str)) {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return;
    }
    unsafe {
        let h = &*hdr_mut();
        let len = (h.len as usize).min(data_cap());
        if len == 0 {
            return;
        }
        let slice = core::slice::from_raw_parts(data_ptr(), len);
        if let Ok(s) = core::str::from_utf8(slice) {
            for line in s.lines().take(max) {
                emit(line);
            }
        }
    }
}

/// Snapshot do conteúdo como String (leitura pública p/ SGDB ingest — IDEA #539c).
/// None se PHYS_MEM_OFFSET ainda 0, buffer vazio ou payload não-UTF8.
pub fn snapshot() -> Option<alloc::string::String> {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return None;
    }
    unsafe {
        let h = &*hdr_mut();
        let len = (h.len as usize).min(data_cap());
        if len == 0 {
            return None;
        }
        let slice = core::slice::from_raw_parts(data_ptr(), len);
        core::str::from_utf8(slice).ok().map(alloc::string::String::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_24_stable_contract_for_logwriter() {
        // Contrato com crates/logwriter-efi — NÃO mudar sem atualizar o EFI.
        assert_eq!(crc32_24(b""), 0);
        assert_eq!(crc32_24(b"NEURLOG!"), 0x00c3_b463);
        assert_eq!(MAGIC_NEED_FLUSH, u64::from_le_bytes(*b"NEURLOG!"));
        assert_eq!(MAGIC_FLUSHED, u64::from_le_bytes(*b"NEURDONE"));
        assert_eq!(BOOT_RAMLOG_PHYS, 0x1000_0000);
        assert_eq!(BOOT_RAMLOG_CAP, 256 * 1024);
        // Layout do header compartilhado com logwriter-efi (HDR_SIZE=24 +
        // flags@16 + recover_count@20). Mudar aqui sem mudar o EFI quebra a
        // captura em silêncio.
        assert_eq!(core::mem::offset_of!(BootRamLogHeader, flags), 16);
        assert_eq!(core::mem::offset_of!(BootRamLogHeader, crc_and_ckpt), 12);
        assert_eq!(core::mem::offset_of!(BootRamLogHeader, recover_count), 20);
        assert_eq!(core::mem::size_of::<BootRamLogHeader>(), 24);
        assert_eq!(FLAG_POWEROFF_AFTER_CAPTURE, 0x504F_4646);
    }

    /// Sem mapping (host / antes de `map_phys`) o selo é no-op honesto —
    /// nunca deref de VA não mapeada. Cobre `seal_now`/quiet/idempotência.
    #[test]
    fn recover_reset_only_bsp_once_and_not_after_pending_seal() {
        assert!(recover_reset_decision(true, false, false));
        assert!(!recover_reset_decision(false, false, false));
        assert!(!recover_reset_decision(true, true, false));
        assert!(!recover_reset_decision(true, false, true));
    }

    /// Missao §6: falha persistente NÃO pode causar reboot infinito — o cap
    /// durável esgota após MAX_RECOVER_REBOOTS e o reset vira park observável.
    #[test]
    fn recover_budget_caps_durable_reboots() {
        assert!(!recover_budget_exhausted(0));
        assert!(!recover_budget_exhausted(MAX_RECOVER_REBOOTS - 1));
        assert!(recover_budget_exhausted(MAX_RECOVER_REBOOTS));
        assert!(recover_budget_exhausted(MAX_RECOVER_REBOOTS + 5));
        assert!(recover_budget_exhausted(u32::MAX)); // saturação: nunca libera
    }

    #[test]
    fn seal_is_safe_noop_without_phys_offset() {
        if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) != 0 {
            return; // outro teste mapeou phys; contrato coberto no bare-metal
        }
        assert!(!seal_for_next_boot());
        assert!(!seal_now());
        assert!(!seal_for_next_boot_quiet());
        assert!(!SEALED.load(Ordering::Acquire));
    }
}
