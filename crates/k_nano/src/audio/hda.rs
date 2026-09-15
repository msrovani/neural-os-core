//! Intel HDA Audio Driver — Capture (SD0 input stream).
//! Implements CORB/RIRB command interface, codec enumeration, SD0 DMA ring (BDL), IRQ handler.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering};
use crate::pci;
use crate::memory::PHYS_MEM_OFFSET;
use crate::dma::{dma_alloc, DmaBuf};
use crate::apic::map_page_uc;
use crate::slog_nano;
use event_bus::{Event, CapabilityToken};

// ============================================================================
// HDA Controller Register Map (Intel HDA 1.0a Spec)
// ============================================================================

// Global Controller Registers
const HDA_GCAP: u64 = 0x00;       // Global Capabilities
const HDA_VMIN: u64 = 0x02;       // Minor Version
const HDA_VMAJ: u64 = 0x03;       // Major Version
const HDA_OUTPAY: u64 = 0x04;     // Output Payload Capability
const HDA_INPAY: u64 = 0x06;      // Input Payload Capability
const HDA_GCTL: u64 = 0x08;       // Global Control
const HDA_WAKEEN: u64 = 0x0C;     // Wake Enable
const HDA_STATESTS: u64 = 0x0E;   // State Change Status
const HDA_GSTS: u64 = 0x10;       // Global Status
const HDA_INTCTL: u64 = 0x20;     // Interrupt Control
const HDA_INTSTS: u64 = 0x24;     // Interrupt Status
const HDA_WALCLK: u64 = 0x30;     // Wall Clock Counter

// CORB (Command Output Ring Buffer)
const HDA_CORBLBASE: u64 = 0x40;  // CORB Lower Base Address
const HDA_CORBUBASE: u64 = 0x44;  // CORB Upper Base Address
const HDA_CORBWP: u64 = 0x48;     // CORB Write Pointer
const HDA_CORBRP: u64 = 0x4A;     // CORB Read Pointer
const HDA_CORBCTL: u64 = 0x4C;    // CORB Control
const HDA_CORBSTS: u64 = 0x4D;    // CORB Status
const HDA_CORBSIZE: u64 = 0x4E;   // CORB Size

// RIRB (Response Input Ring Buffer)
const HDA_RIRBLBASE: u64 = 0x50;  // RIRB Lower Base Address
const HDA_RIRBUBASE: u64 = 0x54;  // RIRB Upper Base Address
const HDA_RIRBWP: u64 = 0x58;     // RIRB Write Pointer
const HDA_RINTCNT: u64 = 0x5A;    // RINTCNT — respostas antes do IRQ (NÃO é read pointer)
const HDA_RIRBCTL: u64 = 0x5C;    // RIRB Control
const HDA_RIRBSTS: u64 = 0x5D;    // RIRB Status
const HDA_RIRBSIZE: u64 = 0x5E;   // RIRB Size

// Immediate Command
const HDA_ICW: u64 = 0x60;        // IC  — Immediate Command (WO, 32b)
const HDA_ICR: u64 = 0x64;        // IR  — Immediate Response (RO, 32b)
const HDA_ICS: u64 = 0x68;        // IRS — Immediate Status (RW, 16b): bit0=BUSY bit1=VALID [7:4]=CAD

// Stream Descriptor 0 (SD0) - Capture (Microphone)
// Base offset 0x80, each SD is 0x20 bytes
const SD0_BASE: u64 = 0x80;
const SDX_CTL: u64 = 0x00;        // Stream Descriptor Control (1 byte)
const SDX_STS: u64 = 0x03;        // Stream Descriptor Status (1 byte)
const SDX_LPIB: u64 = 0x04;       // Link Position in Buffer (4 bytes)
const SDX_CBL: u64 = 0x08;        // Cyclic Buffer Length (4 bytes)
const SDX_LVI: u64 = 0x0C;        // Last Valid Index (2 bytes)
const SDX_FMT: u64 = 0x0E;        // Stream Format (2 bytes)
const SDX_BDPL: u64 = 0x10;       // Buffer Descriptor List Pointer Lower (4 bytes)
const SDX_BDPU: u64 = 0x14;       // Buffer Descriptor List Pointer Upper (4 bytes)

// Stream Descriptor 1 (SD1) - Playback (Speaker)
const SD1_BASE: u64 = 0xA0;

// ============================================================================
// Register Bit Definitions
// ============================================================================

// GCTL
const GCTL_CRST: u32 = 1 << 0;    // Controller Reset
const GCTL_FCNTRL: u32 = 1 << 1;  // Flush Control
const GCTL_UNSOL: u32 = 1 << 8;   // Accept Unsolicited Response Enable

// CORBCTL / RIRBCTL
const CORB_RUN: u32 = 1 << 1;     // Run
const CORB_CMEIE: u32 = 1 << 0;   // CORB Memory Error Interrupt Enable
const RIRB_RINTCTL: u32 = 1 << 0; // Response Interrupt Control
const RIRB_DMA_EN: u32 = 1 << 1;  // DMA Enable

// ICS (0x68) — Immediate Command Status. Layout do silício/QEMU (ICH6_IRS_* em
// `hw/audio/intel-hda-defs.h`, origem linux/sound/pci/hda/hda_intel.c):
//   escrever BUSY(bit0)=1 DISPARA o verbo que está em ICW (não basta escrever ICW);
//   escrever VALID(bit1)=1 LIMPA o resultado; bits[7:4] = CAD que respondeu.
const ICS_BUSY: u16 = 1 << 0;
const ICS_VALID: u16 = 1 << 1;

// RIRBWP/CORBRP bit 15 = reset do ponteiro (self-clearing).
const RIRBWP_RST: u16 = 1 << 15;
const CORBRP_RST: u16 = 1 << 15;

// RIRBSTS (0x5D) — write-1-to-clear.
const RIRBSTS_IRQ: u8 = 1 << 0;
const RIRBSTS_OVERRUN: u8 = 1 << 2;
/// Ambos os anéis (CORB e RIRB) são de 256 entradas — CORBSIZE/RIRBSIZE = 0x02.
const RING_ENTRIES: u32 = 256;

// SDx_CTL
const SD_CTL_RUN: u32 = 1 << 0;   // Run
const SD_CTL_SRST: u32 = 1 << 1;  // Stream Reset
const SD_CTL_IOCE: u32 = 1 << 2;  // Interrupt on Completion Enable
const SD_CTL_FEIE: u32 = 1 << 3;  // FIFO Error Interrupt Enable
const SD_CTL_DEIE: u32 = 1 << 4;  // Descriptor Error Interrupt Enable
/// Stream Number nos bits 19:16 do SDCTL (deve casar com Converter Stream Tag).
const fn sd_ctl_strm(n: u32) -> u32 {
    (n & 0xF) << 16
}
const CAPTURE_STREAM_TAG: u32 = 1;
const PLAYBACK_STREAM_TAG: u32 = 2;

// Amp Gain/Mute payload (HDA §7.3.3.7) — bit7=Mute; L/R/In/Out nos bits altos.
const AMP_SET_OUTPUT: u32 = 1 << 15;
const AMP_SET_INPUT: u32 = 1 << 14;
const AMP_SET_LEFT: u32 = 1 << 13;
const AMP_SET_RIGHT: u32 = 1 << 12;
const AMP_UNMUTE_OUT: u32 = AMP_SET_OUTPUT | AMP_SET_LEFT | AMP_SET_RIGHT;
const AMP_UNMUTE_IN: u32 = AMP_SET_INPUT | AMP_SET_LEFT | AMP_SET_RIGHT;

// SDx_STS
const SD_STS_FIFORDY: u32 = 1 << 0; // FIFO Ready
const SD_STS_BCIS: u32 = 1 << 2;    // Buffer Completion Interrupt Status
const SD_STS_FIFOE: u32 = 1 << 3;   // FIFO Error
const SD_STS_DESE: u32 = 1 << 4;    // Descriptor Error

// ============================================================================
// Codec Verbs (HDA Spec)
// ============================================================================

const VERB_GET_PARAMETER: u32 = 0xF00;
const VERB_GET_CONNECTION_LIST: u32 = 0xF02;
const VERB_GET_CONNECTION_SELECT: u32 = 0xF01;
const VERB_SET_CONNECTION_SELECT: u32 = 0x701;
const VERB_GET_PIN_WIDGET_CONTROL: u32 = 0xF07;
const VERB_SET_PIN_WIDGET_CONTROL: u32 = 0x707;
const VERB_GET_CONVERTER_FORMAT: u32 = 0xA00;
const VERB_SET_CONVERTER_FORMAT: u32 = 0x200;
const VERB_GET_STREAM_FORMAT: u32 = 0xA00;
const VERB_SET_STREAM_FORMAT: u32 = 0x200;
const VERB_GET_AMP_GAIN_MUTE: u32 = 0xB00;
const VERB_SET_AMP_GAIN_MUTE: u32 = 0x300;
const VERB_GET_CONVERTER_STREAM_CHANNEL: u32 = 0xF06;
const VERB_SET_CONVERTER_STREAM_CHANNEL: u32 = 0x706;
const VERB_GET_PIN_SENSE: u32 = 0xF09;
const VERB_GET_CONFIG_DEFAULT: u32 = 0xF1C;
const VERB_GET_SUBSYSTEM_ID: u32 = 0xF20;
const VERB_SET_POWER_STATE: u32 = 0x705;

// Parameter IDs
const PARAM_VENDOR_ID: u32 = 0x00;
const PARAM_REVISION_ID: u32 = 0x02;
const PARAM_SUB_NODE_COUNT: u32 = 0x04;
const PARAM_FUNCTION_GROUP_TYPE: u32 = 0x05;
const PARAM_AUDIO_FG_CAP: u32 = 0x08;
const PARAM_AUDIO_WIDGET_CAP: u32 = 0x09;
const PARAM_PCAP: u32 = 0x0A;
const PARAM_IN_AMP_CAP: u32 = 0x0B;
const PARAM_OUT_AMP_CAP: u32 = 0x0C;
const PARAM_CONNLIST_LEN: u32 = 0x0E;
const PARAM_POWER_STATE: u32 = 0x0F;
const PARAM_PROC_WIDGET_CAP: u32 = 0x10;
/// Get Parameter 0x0A — Supported Stream Formats (só faz sentido em conversores).
const PARAM_SUPP_STREAM_FORMATS: u32 = 0x0A;
const PARAM_GPIO_CAP: u32 = 0x11;
const PARAM_VOLUME_KNOB_CAP: u32 = 0x12;

// Widget Types
const WIDGET_TYPE_AUDIO_OUTPUT: u32 = 0x0;
const WIDGET_TYPE_AUDIO_INPUT: u32 = 0x1;
const WIDGET_TYPE_AUDIO_MIXER: u32 = 0x2;
const WIDGET_TYPE_AUDIO_SELECTOR: u32 = 0x3;
const WIDGET_TYPE_PIN_COMPLEX: u32 = 0x4;
const WIDGET_TYPE_POWER_WIDGET: u32 = 0x5;
const WIDGET_TYPE_VOLUME_KNOB: u32 = 0x6;
const WIDGET_TYPE_BEEP_GENERATOR: u32 = 0x7;
const WIDGET_TYPE_VENDOR_DEFINED: u32 = 0xF;

// Pin Widget Control (HDA 1.0a §7.3.3.13): bit5=In, bit6=Out, bit7=HP, bits2:0=VRef.
const PIN_VREF_HIZ: u32 = 0x00;
const PIN_VREF_50: u32 = 0x01;
const PIN_VREF_GND: u32 = 0x02;
const PIN_VREF_80: u32 = 0x04;
const PIN_VREF_100: u32 = 0x05;
const PIN_IN_EN: u32 = 0x20;
const PIN_OUT_EN: u32 = 0x40;
const PIN_HP_EN: u32 = 0x80;

// ============================================================================
// Audio Format — Intel HDA 1.0a §3.7.1 (Stream Format), layout canônico:
//   bits 14:12 = canais-1 | bits 10:8 = bits/amostra (001=16) |
//   bits  6:4  = base rate (000=48kHz, 101=16kHz) | bits 3:0 = mult/div
//
// FIX (WS1/SESSION_345): o valor anterior era `0x21` — que nesse layout significa
// 8-bit / mono / 32 kHz / NÃO-PCM, isto é, nada do que o comentário dizia. O
// barramento passava a carregar amostras de 8 bits enquanto o driver lia i16 do
// DMA: todo o áudio (captura E playback) era lido com o dobro de duração e metade
// dos canais, além de taxa 32k em vez de 48k. Ver `tools/hda_fmt_check.py`.
// ============================================================================
const FMT_16BIT_48KHZ_STEREO: u32 = 0x0000_1100; // chan(2)=0x1000 | 16-bit=0x100 | base 48k=0
/// 16 kHz mono — usado só quando o codec anuncia suporte (ver `adc_supported_formats`).
const FMT_16BIT_16KHZ_MONO: u32 = 0x0000_0150; // chan(1)=0 | 16-bit=0x100 | base 16k=0x50

/// Formato EFETIVO da captura — fonte única. A FE (jarbas) não deve hardcodar
/// 16 kHz: o ADC entrega 48 kHz estéreo e a conversão vive em `audio/capture.rs`.
pub const CAPTURE_RATE_HZ: u32 = 48000;
pub const CAPTURE_CHANNELS: usize = 2;
/// Taxa consumida pelo pipeline de voz (VAD/STT/SER).
pub const VOICE_RATE_HZ: u32 = 16000;
/// Razão de decimação 48k→16k (fator inteiro, logo a conversão é exata).
pub const VOICE_DECIM: usize = (CAPTURE_RATE_HZ / VOICE_RATE_HZ) as usize;

/// Anel SD0/SD1: 16 descritores de 4 KB = 64 KB.
const BDL_ENTRIES: usize = 16;
const ENTRY_BYTES: usize = 4096;
const SAMPLES_PER_ENTRY: usize = ENTRY_BYTES / 2; // 2048 i16 por entrada

// ============================================================================
// Global State
// ============================================================================

static HDA_INIT_DONE: AtomicBool = AtomicBool::new(false);
static HDA_BAR: AtomicU64 = AtomicU64::new(0);
static HDA_IRQ: AtomicU32 = AtomicU32::new(0);
static HDA_CODEC_MASK: AtomicU32 = AtomicU32::new(0);
static HDA_CORB_BUF: AtomicU64 = AtomicU64::new(0);
static HDA_RIRB_BUF: AtomicU64 = AtomicU64::new(0);
static HDA_SD0_BDL: AtomicU64 = AtomicU64::new(0);
static HDA_SD0_BUF: AtomicU64 = AtomicU64::new(0);
/// Cyclic Buffer Length do SD0 (usado para validar o LPIB no drain).
static HDA_SD0_CBL: AtomicU64 = AtomicU64::new(0);
/// STATE CHange Status — quais slots de codec mudaram de estado (0 = nenhum presente).
static HDA_STATESTS_CACHE: AtomicU32 = AtomicU32::new(0);
/// Tentativas de ICW que não obtiveram resposta (diagnóstico de barramento mudo).
pub static HDA_ICW_FAILS: AtomicU64 = AtomicU64::new(0);
static HDA_CORB_WP: AtomicU32 = AtomicU32::new(0);
static HDA_RIRB_RP: AtomicU32 = AtomicU32::new(0);
static HDA_SD0_RPI: AtomicU32 = AtomicU32::new(0); // Read Pointer Index for BDL
static HDA_SD1_BDL: AtomicU64 = AtomicU64::new(0);
static HDA_SD1_BUF: AtomicU64 = AtomicU64::new(0);
/// Write cursor do playback (entrada BDL corrente) — pagina o anel SD1.
static HDA_SD1_WPI: AtomicU32 = AtomicU32::new(0);

/// O IRQ apenas SINALIZA conclusão; a leitura do BDL + publish acontece no tick do
/// AudioInputAgent. Antes o handler de interrupção fazia o walk completo e alocava
/// um `Vec` por chunk — alloc dentro de IRQ, e o MESMO walk duplicado em dois
/// lugares (duas verdades que divergiam).
static HDA_SD0_PENDING: AtomicBool = AtomicBool::new(false);

/// Contadores auditáveis da captura. `CAP_ENTRIES_DRAINED` deve crescer ~47/s
/// (48 kHz estéreo / 2048 amostras por entrada) e cada entrada é publicada UMA vez.
pub static CAP_ENTRIES_DRAINED: AtomicU64 = AtomicU64::new(0);
pub static CAP_SAMPLES_PUBLISHED: AtomicU64 = AtomicU64::new(0);
/// Incrementa quando o LPIB volta valor implausível e caímos no fallback de 1 entrada.
pub static CAP_LPIB_STALE: AtomicU64 = AtomicU64::new(0);
/// Formatos suportados pelo ADC (verb Get Parameter 0x0A) — observação, não política.
pub static CAP_ADC_SUPPORTED_FMT: AtomicU32 = AtomicU32::new(0);
/// Amostras de playback descartadas por o anel SD1 estar cheio.
pub static PLAY_SAMPLES_DROPPED: AtomicU64 = AtomicU64::new(0);

// DMA buffers (kept alive)
static mut CORB_DMA: Option<DmaBuf> = None;
static mut RIRB_DMA: Option<DmaBuf> = None;
static mut SD0_BDL_DMA: Option<DmaBuf> = None;
static mut SD0_AUDIO_DMA: Option<DmaBuf> = None;
static mut SD1_BDL_DMA: Option<DmaBuf> = None;
static mut SD1_AUDIO_DMA: Option<DmaBuf> = None;

// ============================================================================
// MMIO Access Helpers
// ============================================================================

#[inline]
unsafe fn reg32(bar: u64, off: u64) -> *mut u32 {
    (bar + off) as *mut u32
}

#[inline]
unsafe fn r32(bar: u64, off: u64) -> u32 {
    core::ptr::read_volatile(reg32(bar, off))
}

#[inline]
unsafe fn w32(bar: u64, off: u64, v: u32) {
    core::ptr::write_volatile(reg32(bar, off), v);
}

#[inline]
unsafe fn r16(bar: u64, off: u64) -> u16 {
    core::ptr::read_volatile((bar + off) as *const u16)
}

#[inline]
unsafe fn w16(bar: u64, off: u64, v: u16) {
    core::ptr::write_volatile((bar + off) as *mut u16, v);
}

#[inline]
unsafe fn r8(bar: u64, off: u64) -> u8 {
    core::ptr::read_volatile((bar + off) as *const u8)
}

#[inline]
unsafe fn w8(bar: u64, off: u64, v: u8) {
    core::ptr::write_volatile((bar + off) as *mut u8, v);
}

// ============================================================================
// CORB/RIRB Command Interface
// ============================================================================

/// Allocate and initialize CORB (256 entries) and RIRB (256 entries) in uncached memory.
unsafe fn init_corb_rirb(bar: u64) -> bool {
    // CORB: 256 entries × 4 bytes = 1024 bytes, aligned to 128 bytes
    let corb_size = 256 * 4;
    let corb_dma = match dma_alloc(corb_size) {
        Some(buf) => buf,
        None => {
            slog_nano!("HDA", "error", "Failed to allocate CORB DMA buffer");
            return false;
        }
    };
    let corb_phys = corb_dma.phys;
    HDA_CORB_BUF.store(corb_phys, Ordering::Release);
    
    // RIRB: 256 entries × 8 bytes = 2048 bytes, aligned to 128 bytes
    let rirb_size = 256 * 8;
    let rirb_dma = match dma_alloc(rirb_size) {
        Some(buf) => buf,
        None => {
            slog_nano!("HDA", "error", "Failed to allocate RIRB DMA buffer");
            return false;
        }
    };
    let rirb_phys = rirb_dma.phys;
    HDA_RIRB_BUF.store(rirb_phys, Ordering::Release);
    
    // Store DMA buffers to keep them alive
    CORB_DMA = Some(corb_dma);
    RIRB_DMA = Some(rirb_dma);
    
    // Program CORB base address
    w32(bar, HDA_CORBLBASE, corb_phys as u32);
    w32(bar, HDA_CORBUBASE, (corb_phys >> 32) as u32);
    // Reset do read pointer (§3.4.1): RST=1 é self-clearing. Sem o reset o RP pode
    // herdar lixo e o controlador lê o anel como cheio/vazio errado.
    w16(bar, HDA_CORBRP, CORBRP_RST);
    w16(bar, HDA_CORBRP, 0);
    w16(bar, HDA_CORBWP, 0);
    w8(bar, HDA_CORBSIZE, 0x02); // 256 entries

    // Program RIRB base address. DMA primeiro, CORB_RUN depois: com o CORB já rodando
    // a 1ª resposta chegaria com o RIRB desabilitado e seria descartada.
    w32(bar, HDA_RIRBLBASE, rirb_phys as u32);
    w32(bar, HDA_RIRBUBASE, (rirb_phys >> 32) as u32);
    w16(bar, HDA_RIRBWP, RIRBWP_RST); // reset do write pointer
    // 0x5A é RINTCNT (respostas por IRQ), NÃO um read pointer do RIRB — o RP do RIRB
    // é mantido por software. Escrever 0 aqui = nenhum IRQ de resposta.
    w16(bar, HDA_RINTCNT, 1);
    w8(bar, HDA_RIRBCTL, RIRB_RINTCTL as u8 | RIRB_DMA_EN as u8);
    w8(bar, HDA_RIRBSIZE, 0x02); // 256 entries

    // Habilita o engine CORB por último.
    w8(bar, HDA_CORBCTL, CORB_RUN as u8 | CORB_CMEIE as u8);

    // Estado de software: ambos os ponteiros vêm do device (fonte única de verdade).
    HDA_CORB_WP.store(r16(bar, HDA_CORBWP) as u32, Ordering::Release);
    HDA_RIRB_RP.store(0, Ordering::Release);

    slog_nano!("HDA", "info", "CORB @ 0x{:x} RIRB @ 0x{:x} size={} entries", corb_phys, rirb_phys, RING_ENTRIES);
    true
}

/// Prazo em TEMPO REAL para o handshake com o codec.
///
/// Contagem de spins mente: `for _ in 0..200_000 { r16(...) }` custa ~5 s por
/// timeout no QEMU (cada MMIO é uma saída de VM) e transformou o boot em ~12 min
/// com todas as esperas falhando. Usa `rdtsc` calibrado (lição SESSION_277);
/// sem calibração o teto de iterações continua valendo como rede de segurança.
const HDA_CMD_TIMEOUT_US: u64 = 20_000;
const HDA_CMD_MAX_SPINS: u32 = 1_000_000;

#[inline]
fn hda_deadline() -> u64 {
    let hz = crate::tsc::tsc_hz();
    if hz == 0 {
        return 0; // sem TSC calibrado: só o teto de spins limita
    }
    let ticks = (HDA_CMD_TIMEOUT_US as u128 * hz as u128 / 1_000_000) as u64;
    crate::tsc::rdtsc().wrapping_add(ticks)
}

#[inline]
fn hda_expired(deadline: u64) -> bool {
    deadline != 0 && crate::tsc::rdtsc().wrapping_sub(deadline) < (1u64 << 63)
}

/// Campo Verb+Data (20 bits) do comando HDA na codificação da spec §7.3.1.
///
/// A spec define DUAS formas e o device escolhe pelos 3 bits altos do Verb ID
/// (QEMU `hda_audio_command`: `(data & 0x70000) == 0x70000`):
///   - 12/8: [19:8] = Verb ID, [7:0]  = payload → verbes 0x700-0xFFF (todo GET)
///   - 4/16: [19:8] = Verb ID (byte baixo 0), [15:0] = payload → verbes 0x200/0x300
/// Escrever `VERB | payload` direto (o idioma que existia aqui) coloca o Verb ID
/// em [11:0]: o codec decodifica um verbo inexistente, cai no `default` e responde 0
/// — foi assim que a enumeração "encontrou" um codec com `vendor=0x000000`.
/// Payload e ID são argumentos SEPARADOS porque `SET_CONVERTER_FORMAT` leva 16 bits.
#[inline]
fn verb_field(verb_id: u32, payload: u32) -> u32 {
    let mask = if verb_id & 0x700 != 0 { 0xFF } else { 0xFFFF };
    (verb_id << 8) | (payload & mask)
}

/// Envia um verbo pelo CORB e espera a resposta no RIRB. Retorna (resposta, ok).
///
/// Índices do controlador (QEMU `intel_hda_corb_run` / `intel_hda_response`):
///   - o anel tem comando pendente quando `CORBRP != CORBWP`;
///   - o device CONSOME o slot `CORBRP+1` e só então avança o seu CORBRP;
///   - a resposta é escrita em `RIRBWP+1` e o device avança RIRBWP;
///   - o dword BAIXO da entrada é a resposta, o ALTO é o tag (`solicited|cad`).
/// Escrever/lcr no slot errado (ou ler o dword errado) devolve lixo — três off-by-one
/// que fizeram o caminho CORB nunca aplicar um SET em nenhum codec.
unsafe fn corb_write_and_wait(bar: u64, cad: u8, node: u8, verb_id: u32, payload: u32) -> (u32, bool) {
    let wp = (r16(bar, HDA_CORBWP) as u32) & 0xFF;
    let next_wp = (wp + 1) % RING_ENTRIES;
    let rp = (r16(bar, HDA_CORBRP) as u32) & 0xFF;
    if next_wp == rp {
        return (0, false); // anel cheio
    }

    // Build command: [31:28] = CAD, [27:20] = NodeID, [19:0] = Verb+Data
    let cmd = ((cad as u32) << 28) | ((node as u32) << 20) | verb_field(verb_id, payload);

    // Publica o verbo NO SLOT next_wp antes de mexer no CORBWP (o device lê o slot
    // assim que vê o wp mudar).
    let corb_phys = HDA_CORB_BUF.load(Ordering::Acquire);
    let corb_virt = (corb_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *mut u32;
    core::ptr::write_volatile(corb_virt.add(next_wp as usize), cmd);

    w16(bar, HDA_CORBWP, next_wp as u16);
    HDA_CORB_WP.store(next_wp, Ordering::Release);

    // Espera a resposta (RIRBWP é a fonte de verdade; o RP é de software).
    let deadline = hda_deadline();
    let mut rirb_rp = HDA_RIRB_RP.load(Ordering::Acquire);
    for _ in 0..HDA_CMD_MAX_SPINS {
        let rirb_wp = (r16(bar, HDA_RIRBWP) as u32) & 0xFF;
        if rirb_wp != rirb_rp {
            // O device acabou de escrever em RIRBWP; o slot novo é rirb_rp+1
            // (Linux: `rp = (rp+1) & mask; rb = buf + rp`).
            rirb_rp = (rirb_rp + 1) % RING_ENTRIES;
            let rirb_phys = HDA_RIRB_BUF.load(Ordering::Acquire);
            let rirb_virt = (rirb_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *const u64;
            let entry = core::ptr::read_volatile(rirb_virt.add(rirb_rp as usize));
            HDA_RIRB_RP.store(rirb_rp, Ordering::Release);
            // Limpa RIRBSTS (write-1-to-clear). Com RINTCNT=1 o device PARA de
            // processar o anel CORB enquanto `rirb_count == rirb_cnt`; a limpeza
            // do IRQ é o que reabre o anel (QEMU `intel_hda_set_rirb_sts` →
            // `rirb_count = 0; intel_hda_corb_run()`). Sem isto o caminho CORB
            // trava depois do primeiro verbo e todo SET vira timeout.
            w8(bar, HDA_RIRBSTS, RIRBSTS_IRQ | RIRBSTS_OVERRUN);
            // dword baixo = resposta; alto = tag (bit4 = não-solicitado | CAD).
            return (entry as u32, true);
        }
        if hda_expired(deadline) {
            break;
        }
        core::hint::spin_loop();
    }

    HDA_ICW_FAILS.fetch_add(1, Ordering::Relaxed);
    (0, false) // Timeout
}

/// Envia um verbo pela interface de Immediate Command (usada na enumeração).
///
/// Protocolo (§3.3 + QEMU `intel_hda_set_ics` / `intel_hda_response`):
///   ICW(0x60) recebe o verbo; **ICS(0x68).BUSY=1 é o kick** que faz o device
///   executar; ICS.VALID=1 aparece quando o codec respondeu; a resposta está
///   em IR(0x64). CAD inexistente NUNCA responde — o timeout é o sinal de ausência.
///
/// O código anterior escrevia ICW, lia o próprio ICW (write-only) esperando um bit
/// de busy que mora em OUTRO registrador, e nunca dava o kick: nenhum dos 8 CADs
/// era executado e o boot concluía "No codecs found" (SESSION_346).
unsafe fn icw_send(bar: u64, cad: u8, node: u8, verb_id: u32, payload: u32) -> Option<u32> {
    let icw = ((cad as u32) << 28) | ((node as u32) << 20) | verb_field(verb_id, payload);

    // 1) limpa VALID (write-1-to-clear) para não ler a resposta de um verbo anterior.
    w16(bar, HDA_ICS, ICS_VALID);
    // 2) publica o verbo.
    w32(bar, HDA_ICW, icw);
    // 3) kick.
    w16(bar, HDA_ICS, ICS_BUSY);

    // 4) espera VALID (prazo de parede, não contagem de spins).
    let deadline = hda_deadline();
    for _ in 0..HDA_CMD_MAX_SPINS {
        let st = r16(bar, HDA_ICS);
        if st & ICS_VALID != 0 {
            let resp = r32(bar, HDA_ICR);
            w16(bar, HDA_ICS, ICS_VALID); // limpa para o próximo verbo
            return Some(resp);
        }
        if hda_expired(deadline) {
            break;
        }
        core::hint::spin_loop();
    }

    HDA_ICW_FAILS.fetch_add(1, Ordering::Relaxed);
    None
}

// ============================================================================
// Codec Enumeration & Widget Discovery
// ============================================================================

#[derive(Debug, Clone, Copy)]
struct WidgetInfo {
    nid: u8,
    widget_type: u32,
    caps: u32,
    connections: [u8; 8],
    num_connections: u8,
}

#[derive(Debug, Clone, Copy)]
struct CodecInfo {
    cad: u8,
    vendor_id: u32,
    revision_id: u32,
    widgets: [WidgetInfo; 32],
    num_widgets: u8,
    audio_fg_nid: u8,
    mic_pin_nid: u8,
    adc_nid: u8,
    speaker_pin_nid: u8,
    dac_nid: u8,
}

static mut CODECS: [CodecInfo; 8] = [CodecInfo {
    cad: 0,
    vendor_id: 0,
    revision_id: 0,
    widgets: [WidgetInfo { nid: 0, widget_type: 0, caps: 0, connections: [0; 8], num_connections: 0 }; 32],
    num_widgets: 0,
    audio_fg_nid: 0,
    mic_pin_nid: 0,
    adc_nid: 0,
    speaker_pin_nid: 0,
    dac_nid: 0,
}; 8];

/// Enumerate codecs and discover widgets.
unsafe fn enumerate_codecs(bar: u64) -> bool {
    let mut found_codec = false;
    
    for cad in 0..8u8 {
        // Get vendor ID via Immediate Command (simpler for init)
        let resp = icw_send(bar, cad, 0x00, VERB_GET_PARAMETER, PARAM_VENDOR_ID);
        let vendor_id = match resp {
            Some(v) => v,
            None => continue,
        };
        
        let rev_resp = icw_send(bar, cad, 0x00, VERB_GET_PARAMETER, PARAM_REVISION_ID);
        let revision_id = rev_resp.unwrap_or(0);
        
        slog_nano!("HDA", "info", "Codec {}: vendor={:#08x} rev={:#08x}", cad, vendor_id, revision_id);
        
        // Get sub-node count (widgets)
        let sub_resp = icw_send(bar, cad, 0x00, VERB_GET_PARAMETER, PARAM_SUB_NODE_COUNT);
        let (start_nid, total_widgets) = match sub_resp {
            Some(v) => ((v >> 16) as u8, (v & 0xFF) as u8),
            None => continue,
        };
        
        let mut codec = CodecInfo {
            cad,
            vendor_id,
            revision_id,
            widgets: [WidgetInfo { nid: 0, widget_type: 0, caps: 0, connections: [0; 8], num_connections: 0 }; 32],
            num_widgets: 0,
            audio_fg_nid: 0,
            mic_pin_nid: 0,
            adc_nid: 0,
            speaker_pin_nid: 0,
            dac_nid: 0,
        };
        
        // Enumerate widgets
        let mut widget_idx = 0;
        for nid in start_nid..(start_nid + total_widgets) {
            if widget_idx >= 32 { break; }
            
            // Get widget capabilities
            let caps_resp = icw_send(bar, cad, nid, VERB_GET_PARAMETER, PARAM_AUDIO_WIDGET_CAP);
            let caps = caps_resp.unwrap_or(0);
            let widget_type = (caps >> 20) & 0xF;
            
            let mut widget = WidgetInfo {
                nid,
                widget_type,
                caps,
                connections: [0; 8],
                num_connections: 0,
            };
            
            // Get connection list for input widgets
            if widget_type == WIDGET_TYPE_AUDIO_INPUT || widget_type == WIDGET_TYPE_PIN_COMPLEX {
                let conn_resp = icw_send(bar, cad, nid, VERB_GET_PARAMETER, PARAM_CONNLIST_LEN);
                if let Some(conn_len) = conn_resp {
                    let num_conns = (conn_len & 0x7F) as u8;
                    widget.num_connections = num_conns.min(8);
                    
                    // Read connection list (long form if > 8)
                    if num_conns > 0 {
                        let list_resp = icw_send(bar, cad, nid, VERB_GET_CONNECTION_LIST, 0);
                        if let Some(list) = list_resp {
                            for i in 0..widget.num_connections as usize {
                                widget.connections[i] = ((list >> (i * 4)) & 0xF) as u8;
                            }
                        }
                    }
                }
            }
            
            // Check for Audio Function Group
            if widget_type == 0x1 { // Function group
                let fg_type_resp = icw_send(bar, cad, nid, VERB_GET_PARAMETER, PARAM_FUNCTION_GROUP_TYPE);
                if let Some(fg_type) = fg_type_resp {
                    if fg_type & 0xFF == 0x01 { // Audio Function Group
                        codec.audio_fg_nid = nid;
                    }
                }
            }
            
            // Pin Complex: mic (IN) e speaker/HP (OUT) via PCAP + Config Default.
            // PCAP §7.3.4.9: bit4=OutputCapable, bit5=InputCapable.
            // Config Default device §7.3.3.31: 0=LineOut 1=Speaker 2=HP 4=Mic 0xA=LineIn.
            if widget_type == WIDGET_TYPE_PIN_COMPLEX {
                let pin_cap = icw_send(bar, cad, nid, VERB_GET_PARAMETER, PARAM_PCAP).unwrap_or(0);
                let input_cap = (pin_cap >> 5) & 1;
                let output_cap = (pin_cap >> 4) & 1;
                let cfg = icw_send(bar, cad, nid, VERB_GET_CONFIG_DEFAULT, 0).unwrap_or(0);
                let device = (cfg >> 20) & 0xF;
                if input_cap == 1 {
                    if device == 0x4 || codec.mic_pin_nid == 0 {
                        codec.mic_pin_nid = nid;
                    } else if device == 0xA && codec.mic_pin_nid == 0 {
                        codec.mic_pin_nid = nid;
                    }
                }
                if output_cap == 1 {
                    if device == 0x1 || codec.speaker_pin_nid == 0 {
                        codec.speaker_pin_nid = nid;
                    } else if (device == 0x2 || device == 0x0) && codec.speaker_pin_nid == 0 {
                        codec.speaker_pin_nid = nid;
                    }
                }
            }

            // ADC (Audio Input)
            if widget_type == WIDGET_TYPE_AUDIO_INPUT {
                if codec.mic_pin_nid != 0 {
                    for i in 0..widget.num_connections as usize {
                        if widget.connections[i] == codec.mic_pin_nid {
                            codec.adc_nid = nid;
                            break;
                        }
                    }
                }
                if codec.adc_nid == 0 {
                    codec.adc_nid = nid;
                }
            }

            // DAC (Audio Output) — fallback; refine após enum via speaker pin.
            if widget_type == WIDGET_TYPE_AUDIO_OUTPUT && codec.dac_nid == 0 {
                codec.dac_nid = nid;
            }
            
            codec.widgets[widget_idx] = widget;
            widget_idx += 1;
        }
        
        // O log é o único canal de diagnóstico visível no metal: sem isto não se
        // distingue "nó inexistente", "CAD mudo" e "verbo errado" — foi essa
        // cegueira que deixou o capture path falhando sem explicação.
        slog_nano!(
            "HDA",
            "info",
            "enum cad={} start_nid={} total={} widgets={} fg={} mic_pin={} adc={} spk={} dac={} icw_fails={}",
            cad,
            start_nid,
            total_widgets,
            widget_idx,
            codec.audio_fg_nid,
            codec.mic_pin_nid,
            codec.adc_nid,
            codec.speaker_pin_nid,
            codec.dac_nid,
            HDA_ICW_FAILS.load(Ordering::Relaxed)
        );

        codec.num_widgets = widget_idx as u8;
        resolve_dac_from_speaker(&mut codec);
        CODECS[cad as usize] = codec;
        HDA_CODEC_MASK.fetch_or(1 << cad, Ordering::Release);
        found_codec = true;
    }
    
    found_codec
}

fn find_widget(codec: &CodecInfo, nid: u8) -> Option<&WidgetInfo> {
    codec.widgets[..codec.num_widgets as usize]
        .iter()
        .find(|w| w.nid == nid)
}

fn connection_index(w: &WidgetInfo, target_nid: u8) -> u32 {
    for i in 0..w.num_connections as usize {
        if w.connections[i] == target_nid {
            return i as u32;
        }
    }
    0
}

/// Speaker/HP pin → DAC (via mixer/selector se necessário).
fn resolve_dac_from_speaker(codec: &mut CodecInfo) {
    let pin = codec.speaker_pin_nid;
    if pin == 0 {
        return;
    }
    let Some(pin_w) = find_widget(codec, pin) else {
        return;
    };
    let conns: [u8; 8] = pin_w.connections;
    let n = pin_w.num_connections as usize;
    for i in 0..n {
        let nid = conns[i];
        if let Some(w) = find_widget(codec, nid) {
            if w.widget_type == WIDGET_TYPE_AUDIO_OUTPUT {
                codec.dac_nid = nid;
                return;
            }
            if w.widget_type == WIDGET_TYPE_AUDIO_MIXER
                || w.widget_type == WIDGET_TYPE_AUDIO_SELECTOR
            {
                for j in 0..w.num_connections as usize {
                    let dn = w.connections[j];
                    if let Some(dw) = find_widget(codec, dn) {
                        if dw.widget_type == WIDGET_TYPE_AUDIO_OUTPUT {
                            codec.dac_nid = dn;
                            return;
                        }
                    }
                }
            }
        }
    }
}

/// Configure the microphone pin and ADC for capture.
unsafe fn configure_capture_path(bar: u64) -> bool {
    for cad in 0..8u8 {
        if HDA_CODEC_MASK.load(Ordering::Acquire) & (1 << cad) == 0 {
            continue;
        }

        let codec = CODECS[cad as usize];
        if codec.mic_pin_nid == 0 || codec.adc_nid == 0 {
            continue;
        }

        slog_nano!(
            "HDA",
            "ok",
            "capture CAD={} PIN={} ADC={}",
            cad,
            codec.mic_pin_nid,
            codec.adc_nid
        );

        if codec.audio_fg_nid != 0 {
            let _ = corb_write_and_wait(bar, cad, codec.audio_fg_nid, VERB_SET_POWER_STATE, 0);
        }

        // Pin Widget Control: IN_EN + VREF 80% (HDA 1.0a bits).
        let pin_ctl = PIN_VREF_80 | PIN_IN_EN;
        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.mic_pin_nid,
            VERB_SET_PIN_WIDGET_CONTROL,
            pin_ctl,
        );

        // Connection Select = índice na lista do ADC, NÃO o NID.
        let conn_idx = find_widget(&codec, codec.adc_nid)
            .map(|w| connection_index(w, codec.mic_pin_nid))
            .unwrap_or(0);
        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.adc_nid,
            VERB_SET_CONNECTION_SELECT,
            conn_idx,
        );

        // OBSERVAÇÃO (não política): o que o ADC realmente anuncia suportar.
        // Bit 0 = 8 kHz, 1 = 11.025, 2 = 16 kHz, 3 = 22.05, 4 = 24, 5 = 32,
        // 6 = 44.1, 7 = 48 kHz; bits 8–10 = 16/20/24/32-bit; 11–13 = taxas ×2..×4.
        // Fica registrado para decidir o switch gated para 16 kHz mono em HW real.
        if let Some(fmt) = icw_send(bar, cad, codec.adc_nid, VERB_GET_PARAMETER, PARAM_SUPP_STREAM_FORMATS) {
            CAP_ADC_SUPPORTED_FMT.store(fmt, Ordering::Release);
            let sixteen_k = fmt & (1 << 2) != 0;
            slog_nano!(
                "HDA",
                "info",
                "ADC fmt cap=0x{:08X} 16k={} (fmt ativo=0x{:04X} 48k/est) mono16k=0x{:04X} gated",
                fmt,
                sixteen_k,
                FMT_16BIT_48KHZ_STEREO,
                FMT_16BIT_16KHZ_MONO
            );
        }

        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.adc_nid,
            VERB_SET_CONVERTER_FORMAT,
            FMT_16BIT_48KHZ_STEREO,
        );

        let stream_channel = (CAPTURE_STREAM_TAG << 4) | 0;
        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.adc_nid,
            VERB_SET_CONVERTER_STREAM_CHANNEL,
            stream_channel,
        );

        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.mic_pin_nid,
            VERB_SET_AMP_GAIN_MUTE,
            AMP_UNMUTE_IN,
        );
        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.adc_nid,
            VERB_SET_AMP_GAIN_MUTE,
            AMP_UNMUTE_IN,
        );

        slog_nano!("HDA", "ok", "capture path ready CAD {}", cad);
        return true;
    }

    false
}

/// Speaker/HP pin + DAC → stream tag de playback (SD1).
unsafe fn configure_playback_path(bar: u64) -> bool {
    for cad in 0..8u8 {
        if HDA_CODEC_MASK.load(Ordering::Acquire) & (1 << cad) == 0 {
            continue;
        }

        let codec = CODECS[cad as usize];
        if codec.speaker_pin_nid == 0 || codec.dac_nid == 0 {
            continue;
        }

        slog_nano!(
            "HDA",
            "ok",
            "playback CAD={} PIN={} DAC={}",
            cad,
            codec.speaker_pin_nid,
            codec.dac_nid
        );

        // OUT_EN + HP_EN (seguro em speaker e headphone).
        let pin_ctl = PIN_OUT_EN | PIN_HP_EN;
        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.speaker_pin_nid,
            VERB_SET_PIN_WIDGET_CONTROL,
            pin_ctl,
        );

        // Se o pin tem lista, seleciona DAC/mixer conectado.
        if let Some(pin_w) = find_widget(&codec, codec.speaker_pin_nid) {
            if pin_w.num_connections > 0 {
                let idx = connection_index(pin_w, codec.dac_nid);
                // Se DAC não está direto, idx=0 (primeiro conn = mixer típico).
                let _ = corb_write_and_wait(
                    bar,
                    cad,
                    codec.speaker_pin_nid,
                    VERB_SET_CONNECTION_SELECT,
                    idx,
                );
            }
        }

        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.dac_nid,
            VERB_SET_CONVERTER_FORMAT,
            FMT_16BIT_48KHZ_STEREO,
        );
        let stream_channel = (PLAYBACK_STREAM_TAG << 4) | 0;
        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.dac_nid,
            VERB_SET_CONVERTER_STREAM_CHANNEL,
            stream_channel,
        );

        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.dac_nid,
            VERB_SET_AMP_GAIN_MUTE,
            AMP_UNMUTE_OUT,
        );
        let _ = corb_write_and_wait(
            bar,
            cad,
            codec.speaker_pin_nid,
            VERB_SET_AMP_GAIN_MUTE,
            AMP_UNMUTE_OUT,
        );

        slog_nano!("HDA", "ok", "playback path ready CAD {}", cad);
        return true;
    }

    false
}

// ============================================================================
// SD0 Input Stream Setup (BDL - Buffer Descriptor List)
// ============================================================================

/// Allocate and configure SD0 BDL and audio buffer.
/// BDL: 16 entries × 16 bytes = 256 bytes (each entry: 8-byte addr + 4-byte len + 4-byte IOC)
/// Audio buffer: 16 × 4KB = 64KB ring
unsafe fn init_sd0_capture(bar: u64) -> bool {
    // Allocate BDL (16 entries, 16 bytes each = 256 bytes, aligned to 128 bytes)
    let bdl_dma = match dma_alloc(256) {
        Some(buf) => buf,
        None => {
            slog_nano!("HDA", "error", "Failed to allocate SD0 BDL");
            return false;
        }
    };
    let bdl_phys = bdl_dma.phys;
    HDA_SD0_BDL.store(bdl_phys, Ordering::Release);
    SD0_BDL_DMA = Some(bdl_dma);
    
    // Allocate audio capture buffer: 16 pages × 4KB = 64KB
    let audio_size = 16 * 4096;
    let audio_dma = match dma_alloc(audio_size) {
        Some(buf) => buf,
        None => {
            slog_nano!("HDA", "error", "Failed to allocate SD0 audio buffer");
            return false;
        }
    };
    let audio_phys = audio_dma.phys;
    HDA_SD0_BUF.store(audio_phys, Ordering::Release);
    SD0_AUDIO_DMA = Some(audio_dma);
    
    // Build BDL entries: 16 entries of 4KB each
    // BDL entry format (16 bytes): u64 addr + u32 len + u32 ioc
    let bdl_virt = (bdl_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *mut u8;
    for i in 0..16 {
        let entry_phys = audio_phys + (i as u64 * 4096);
        let entry_base = bdl_virt.add(i * 16);
        // Buffer address (8 bytes)
        unsafe { core::ptr::write_volatile(entry_base as *mut u64, entry_phys); }
        // Buffer length (4 bytes) - 4096 bytes per entry
        unsafe { core::ptr::write_volatile(entry_base.add(8) as *mut u32, 4096u32); }
        // IOC (4 bytes) - bit 0 = interrupt on completion
        unsafe { core::ptr::write_volatile(entry_base.add(12) as *mut u32, 1u32); }
    }
    
    // Program SD0 registers
    let sd0_ctl = SD0_BASE + SDX_CTL;
    let sd0_sts = SD0_BASE + SDX_STS;
    let sd0_cbl = SD0_BASE + SDX_CBL;
    let sd0_lvi = SD0_BASE + SDX_LVI;
    let sd0_fmt = SD0_BASE + SDX_FMT;
    let sd0_bdpl = SD0_BASE + SDX_BDPL;
    let sd0_bdpu = SD0_BASE + SDX_BDPU;
    
    // Stop stream first
    w8(bar, sd0_ctl, 0);
    for _ in 0..1000 { core::hint::spin_loop(); }
    
    // Reset stream
    w8(bar, sd0_ctl, SD_CTL_SRST as u8);
    for _ in 0..1000 { core::hint::spin_loop(); }
    w8(bar, sd0_ctl, 0);
    for _ in 0..1000 { core::hint::spin_loop(); }
    
    // Program BDL address
    w32(bar, sd0_bdpl, bdl_phys as u32);
    w32(bar, sd0_bdpu, (bdl_phys >> 32) as u32);
    
    // Program Cyclic Buffer Length (total ring size = 64KB)
    w32(bar, sd0_cbl, audio_size as u32);
    HDA_SD0_CBL.store(audio_size as u64, Ordering::Release);
    
    // Program Last Valid Index (15 = 16 entries, 0-based)
    w16(bar, sd0_lvi, 15);
    
    // Program Format (16-bit, 48kHz, stereo)
    w16(bar, sd0_fmt, FMT_16BIT_48KHZ_STEREO as u16);
    
    // Clear status
    w16(bar, sd0_sts, 0xFFFF); // Write 1 to clear
    
    // Enable stream: STRM=1 (casa com ADC tag) + RUN + IOCE
    w32(
        bar,
        sd0_ctl,
        sd_ctl_strm(CAPTURE_STREAM_TAG) | SD_CTL_RUN | SD_CTL_IOCE,
    );
    
    // Wait for FIFO ready
    for _ in 0..10000 {
        let sts = r8(bar, sd0_sts);
        if sts & SD_STS_FIFORDY as u8 != 0 {
            break;
        }
        core::hint::spin_loop();
    }
    
    // Reset read pointer index
    HDA_SD0_RPI.store(0, Ordering::Release);
    
    slog_nano!("HDA", "info", "SD0 capture: BDL @ 0x{:x} buf @ 0x{:x} size={}KB", bdl_phys, audio_phys, audio_size / 1024);
    true
}

/// SD1 playback BDL (mesmo layout do SD0). Falha não aborta o bring-up de captura.
unsafe fn init_sd1_playback(bar: u64) -> bool {
    let bdl_dma = match dma_alloc(256) {
        Some(buf) => buf,
        None => return false,
    };
    let bdl_phys = bdl_dma.phys;
    HDA_SD1_BDL.store(bdl_phys, Ordering::Release);
    SD1_BDL_DMA = Some(bdl_dma);

    let audio_size = 16 * 4096;
    let audio_dma = match dma_alloc(audio_size) {
        Some(buf) => buf,
        None => return false,
    };
    let audio_phys = audio_dma.phys;
    HDA_SD1_BUF.store(audio_phys, Ordering::Release);
    SD1_AUDIO_DMA = Some(audio_dma);

    let bdl_virt = (bdl_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *mut u8;
    for i in 0..16 {
        let entry_phys = audio_phys + (i as u64 * 4096);
        let entry_base = bdl_virt.add(i * 16);
        core::ptr::write_volatile(entry_base as *mut u64, entry_phys);
        core::ptr::write_volatile(entry_base.add(8) as *mut u32, 4096u32);
        core::ptr::write_volatile(entry_base.add(12) as *mut u32, 1u32);
    }

    let ctl = SD1_BASE + SDX_CTL;
    let sts = SD1_BASE + SDX_STS;
    let cbl = SD1_BASE + SDX_CBL;
    let lvi = SD1_BASE + SDX_LVI;
    let fmt = SD1_BASE + SDX_FMT;
    let bdpl = SD1_BASE + SDX_BDPL;
    let bdpu = SD1_BASE + SDX_BDPU;

    w8(bar, ctl, 0);
    for _ in 0..1000 { core::hint::spin_loop(); }
    w8(bar, ctl, SD_CTL_SRST as u8);
    for _ in 0..1000 { core::hint::spin_loop(); }
    w8(bar, ctl, 0);
    for _ in 0..1000 { core::hint::spin_loop(); }

    w32(bar, bdpl, bdl_phys as u32);
    w32(bar, bdpu, (bdl_phys >> 32) as u32);
    w32(bar, cbl, audio_size as u32);
    w16(bar, lvi, 15);
    w16(bar, fmt, FMT_16BIT_48KHZ_STEREO as u16);
    w16(bar, sts, 0xFFFF);
    // STRM=2 casa com DAC Converter Stream Tag
    w32(
        bar,
        ctl,
        sd_ctl_strm(PLAYBACK_STREAM_TAG) | SD_CTL_RUN | SD_CTL_IOCE,
    );

    slog_nano!("HDA", "info", "SD1 playback: BDL @ 0x{:x} buf @ 0x{:x}", bdl_phys, audio_phys);
    true
}

// ============================================================================
// IRQ Handler
// ============================================================================

/// HDA interrupt handler - called from interrupts.rs
/// Minimal work: copy completed BDL entries to MIC_CAPTURE_RING, advance RPI
pub unsafe fn hda_irq_handler() {
    let bar = HDA_BAR.load(Ordering::Acquire);
    if bar == 0 || !HDA_INIT_DONE.load(Ordering::Acquire) {
        return;
    }
    
    // Check global interrupt status
    let intsts = r32(bar, HDA_INTSTS);
    if intsts == 0 {
        return;
    }
    
    // Check SD0 interrupt status
    let sd0_sts_off = SD0_BASE + SDX_STS;
    let sd0_sts = r16(bar, sd0_sts_off);
    
    // Clear interrupt (write 1 to clear BCIS)
    if sd0_sts & SD_STS_BCIS as u16 != 0 {
        w16(bar, sd0_sts_off, sd0_sts | SD_STS_BCIS as u16);
    }
    
    // Check for FIFO error or descriptor error
    if sd0_sts & (SD_STS_FIFOE | SD_STS_DESE) as u16 != 0 {
        slog_nano!("HDA", "warn", "SD0 error: sts={:#06x}", sd0_sts);
        // Clear errors
        w16(bar, sd0_sts_off, sd0_sts | SD_STS_FIFOE as u16 | SD_STS_DESE as u16);
    }
    
    // Interrupção NÃO faz trabalho pesado: sem alloc, sem publish, sem walk do BDL
    // (era aqui que o mesmo descritor era republicado 16× — ver drain_sd0_completed).
    HDA_SD0_PENDING.store(true, Ordering::Release);

    // Acknowledge global interrupt (write 1 to clear)
    w32(bar, HDA_INTSTS, intsts);
}

/// Publica UMA entrada do BDL SD0 (2048 i16 = 4 KB @48 kHz estéreo ≈ 21 ms).
///
/// Antes: cada entrada era fatiada em 4 publishes de 512 amostras e o mesmo índice
/// era republicado 16× por interrupção (`rpi` era capturado FORA do loop, então
/// `entry_idx` e `next_rpi` nunca mudavam) → 32.768 amostras duplicadas por BCIS e
/// um RPI que avançava 1 enquanto o hardware já tinha passado por N entradas.
unsafe fn publish_sd0_entry(entry_idx: usize) {
    let audio_phys = HDA_SD0_BUF.load(Ordering::Acquire);
    if audio_phys == 0 {
        return;
    }
    let audio_virt = (audio_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *const i16;
    let samples = core::slice::from_raw_parts(audio_virt.add(entry_idx * SAMPLES_PER_ENTRY), SAMPLES_PER_ENTRY);
    let mut buf = alloc::vec::Vec::with_capacity(SAMPLES_PER_ENTRY * 2);
    for &s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    let _ = crate::globals::EVENT_BUS.publish(Event {
        id: 0,
        topic: alloc::string::String::from("AUDIO_IN"),
        payload: buf,
        token: CapabilityToken::Legacy(1),
    });
    CAP_SAMPLES_PUBLISHED.fetch_add(SAMPLES_PER_ENTRY as u64, Ordering::Relaxed);
}

/// Drena as entradas do BDL SD0 que o controlador JÁ completou, uma única vez cada.
///
/// A posição corrente do hardware vem do LPIB (Link Position In Buffer, offset
/// dentro do buffer cíclico). Entradas entre o nosso RPI e a entrada corrente do
/// hardware estão completas. LPIB implausível → fallback honesto de 1 entrada por
/// chamada (nunca trava, nunca republica em loop).
pub unsafe fn drain_sd0_completed() {
    let bar = HDA_BAR.load(Ordering::Acquire);
    if bar == 0 || !HDA_INIT_DONE.load(Ordering::Acquire) {
        return;
    }
    let sts_off = SD0_BASE + SDX_STS;
    let sts = r16(bar, sts_off);
    let pending = HDA_SD0_PENDING.swap(false, Ordering::AcqRel);
    if !pending && sts & SD_STS_BCIS as u16 == 0 {
        return;
    }
    if sts & SD_STS_BCIS as u16 != 0 {
        w16(bar, sts_off, sts | SD_STS_BCIS as u16);
    }

    let lpib = r32(bar, SD0_BASE + SDX_LPIB) as usize;
    let cbl = HDA_SD0_CBL.load(Ordering::Acquire) as usize;
    let mut rpi = HDA_SD0_RPI.load(Ordering::Acquire);

    // Quantas entradas completas drenar. `hw_entry` é a entrada que o hardware está
    // escrevendo AGORA — nunca a publicamos (ainda incompleta).
    let valid_lpib = cbl > 0 && lpib < cbl;
    let hw_entry = if valid_lpib { (lpib / ENTRY_BYTES) % BDL_ENTRIES } else { 0 };
    if !valid_lpib {
        CAP_LPIB_STALE.fetch_add(1, Ordering::Relaxed);
    }

    let mut drained = 0usize;
    loop {
        if drained >= BDL_ENTRIES {
            break;
        }
        if valid_lpib {
            // RPI alcançou a posição do hardware: nada novo completo. Não republica.
            if rpi as usize == hw_entry {
                break;
            }
        } else if drained >= 1 {
            break; // LPIB implausível: degrada para 1 entrada por chamada
        }
        publish_sd0_entry(rpi as usize);
        rpi = (rpi + 1) % BDL_ENTRIES as u32;
        drained += 1;
    }
    if drained > 0 {
        HDA_SD0_RPI.store(rpi, Ordering::Release);
        CAP_ENTRIES_DRAINED.fetch_add(drained as u64, Ordering::Relaxed);
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Initialize HDA controller: PCI discovery, CORB/RIRB, codec enumeration, SD0 setup.
/// Returns true on success.
pub fn init_hda() -> bool {
    if HDA_INIT_DONE.load(Ordering::Acquire) {
        return true;
    }
    
    slog_nano!("HDA", "info", "Initializing Intel HDA capture driver...");
    
    // Scan PCI for HDA controller (class 0x04, subclass 0x03)
    let devices = unsafe { pci::scan_pci() };
    let mut hda_dev = None;
    
    for dev in &devices {
        if dev.class == 0x04 && dev.subclass == 0x03 {
            hda_dev = Some(*dev);
            break;
        }
    }
    
    let dev = match hda_dev {
        Some(d) => d,
        None => {
            slog_nano!("HDA", "warn", "No Intel HDA controller found");
            return false;
        }
    };
    
    slog_nano!("HDA", "info", "Found HDA: {:04x}:{:04x} bus={} dev={} fn={} BAR0={:#x} IRQ={}",
        dev.vendor_id, dev.device_id, dev.bus, dev.device, dev.function, dev.bar0, dev.prog_if);
    
    // Enable PCI Bus Master + Memory Space
    unsafe { pci::enable_pci_bus_master(&dev); }
    
    // Get physical memory offset
    let pm_off = PHYS_MEM_OFFSET.load(Ordering::Acquire);
    if pm_off == 0 {
        slog_nano!("HDA", "error", "PHYS_MEM_OFFSET not set");
        return false;
    }
    
    // Map BAR0 MMIO as uncacheable
    let bar_phys = dev.bar0 & !0xF;
    let bar = bar_phys + pm_off;
    
    // Map all pages of BAR0 (typically 16KB = 4 pages)
    for i in 0..4 {
        unsafe { map_page_uc(bar_phys + i * 4096, pm_off); }
    }
    
    HDA_BAR.store(bar, Ordering::Release);
    
    // Reset do controlador — GCTL.CRST: **0 = em reset, 1 = fora de reset**
    // (Intel HDA 1.0a §4.3, GCTL bit 0).
    //
    // FIX (SESSION_346): a sequência anterior fazia `CRST=1` e depois `CRST=0` e ainda
    // *validava* `CRST == 0` como sucesso — ou seja, deixava o controlador EM RESET e
    // seguia usando ICW/CORB/RIRB (registradores que só vivem fora do reset). Resultado:
    // `icw_send` devolvia `None` para todo CAD, `enumerate_codecs` achava zero codecs, e a
    // conclusão registrada na SESSION_286 ("QEMU intel-hda: CORB/RIRB frequentemente mudo;
    // aceite = HW real") era um SINTOMA deste bug, não uma limitação do emulador. Com o
    // controlador em reset, nenhum áudio funcionava em lugar nenhum (QEMU ou metal).
    unsafe {
        // 1) entra em reset e espera o bit BAIXAR
        w32(bar, HDA_GCTL, r32(bar, HDA_GCTL) & !GCTL_CRST);
        let mut entered = false;
        for _ in 0..500_000 {
            if r32(bar, HDA_GCTL) & GCTL_CRST == 0 {
                entered = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !entered {
            slog_nano!("HDA", "error", "GCTL.CRST nao desceu — controlador nao entrou em reset");
            return false;
        }

        // 2) sai do reset e espera o bit SUBIR (é isso que significa "out of reset")
        w32(bar, HDA_GCTL, r32(bar, HDA_GCTL) | GCTL_CRST);
        let mut out_of_reset = false;
        for _ in 0..500_000 {
            if r32(bar, HDA_GCTL) & GCTL_CRST != 0 {
                out_of_reset = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !out_of_reset {
            slog_nano!("HDA", "error", "GCTL.CRST nao subiu — controlador permanece em reset");
            return false;
        }

        // 3) codecs precisam de alguns ms para reportar presença depois do reset.
        // Antes o código só girava `spin_loop` 10k vezes (~10 µs) e nunca lia STATESTS.
        for _ in 0..2_000_000 { core::hint::spin_loop(); }
        let statests = r16(bar, HDA_STATESTS);
        HDA_STATESTS_CACHE.store(statests as u32, Ordering::Release);
        // "instrumento invisível = instrumento inexistente": o estado do barramento de
        // codecs passa a ser registrado (é o que distingue "sem codec" de "driver mudo").
        slog_nano!("HDA", "info", "CRST ok (fora de reset) STATESTS=0x{:04x}", statests);

        // Enable unsolicited responses
        let gctl = r32(bar, HDA_GCTL);
        w32(bar, HDA_GCTL, gctl | GCTL_UNSOL);
        
        // Initialize CORB/RIRB
        if !init_corb_rirb(bar) {
            return false;
        }
        
        // Enumerate codecs and discover widgets
        if !enumerate_codecs(bar) {
            let profile = if crate::platform_probe::probe_done()
                && crate::platform_probe::hypervisor().is_sandbox()
            {
                "qemu"
            } else {
                "hw"
            };
            // Diagnóstico honesto: STATESTS diz se ALGUM codec se apresentou no barramento
            // e HDA_ICW_FAILS conta comandos que ficaram sem resposta. "Nenhum codec no
            // barramento" e "barramento mudo" são causas diferentes — a conclusão anterior
            // (SESSION_286: "QEMU CORB/RIRB mudo") foi tirada sem essa evidência, e o motivo
            // real era o CRST invertido que deixava o controlador em reset.
            let statests = HDA_STATESTS_CACHE.load(Ordering::Acquire);
            slog_nano!(
                "HDA",
                "warn",
                "home=k_nano::audio::hda profile={} | No codecs found (STATESTS=0x{:04x} icw_fails={}){} — aceite=HW",
                profile,
                statests,
                HDA_ICW_FAILS.load(Ordering::Relaxed),
                if statests == 0 {
                    " [barramento sem codec presente]"
                } else {
                    " [codec presente, enumeracao falhou]"
                }
            );
            return false;
        }
        
        // Configure capture path (mic pin + ADC)
        if !configure_capture_path(bar) {
            slog_nano!("HDA", "warn", "Failed to configure capture path");
            return false;
        }
        
        // Initialize SD0 capture stream
        if !init_sd0_capture(bar) {
            return false;
        }

        // Playback: unmute DAC/speaker ANTES de armar SD1.
        if !configure_playback_path(bar) {
            slog_nano!("HDA", "warn", "playback path absent — TTS formant-only / no speaker");
        }
        if !init_sd1_playback(bar) {
            slog_nano!("HDA", "warn", "SD1 playback not armed — write_hda_playback no-op");
        }
        
        // Enable global interrupts
        w32(bar, HDA_INTCTL, 0xFFFF_FFFF); // Enable all stream interrupts
        w32(bar, HDA_INTCTL, r32(bar, HDA_INTCTL) | 1); // Global interrupt enable
    }
    
    HDA_INIT_DONE.store(true, Ordering::Release);
    slog_nano!("HDA", "info", "Intel HDA capture driver initialized successfully");
    true
}

/// Poll HDA audio (compatibility function for non-IRQ path).
/// Reads completed BDL entries and publishes to EventBus.
pub fn poll_hda_audio() {
    // Uma única verdade do walk do BDL (a mesma usada pelo IRQ handler).
    unsafe {
        drain_sd0_completed();
    }
}

/// IRQ+MMIO já programados por `init_hda` (mesmo BAR do IDT 0x30).
pub fn is_ready() -> bool {
    HDA_INIT_DONE.load(Ordering::Acquire) && HDA_BAR.load(Ordering::Acquire) != 0
}

/// Escreve samples no BDL SD1 (mesma instância do IRQ/captura). No-op se SD1 não armou.
pub fn write_hda_playback(samples: &[i16]) {
    if !is_ready() || samples.is_empty() {
        return;
    }
    let bar = HDA_BAR.load(Ordering::Acquire);
    let audio_phys = HDA_SD1_BUF.load(Ordering::Acquire);
    if bar == 0 || audio_phys == 0 {
        return;
    }
    unsafe {
        let sts_off = SD1_BASE + SDX_STS;
        let sts = r16(bar, sts_off);
        if sts & SD_STS_BCIS as u16 != 0 {
            w16(bar, sts_off, sts | SD_STS_BCIS as u16);
        }

        // FIX (WS1): o playback escrevia PCM MONO @16 kHz direto num stream
        // configurado como 48 kHz ESTÉREO — cada amostra virava um canal e a taxa
        // efetiva ficava 6× acima (L/R alternados, sem interpolação). Agora expande
        // mono→estéreo (L=R) e interpola por VOICE_DECIM (hold), que é o inverso
        // exato da decimação de `audio/capture.rs`.
        let virt = (audio_phys + PHYS_MEM_OFFSET.load(Ordering::Acquire)) as *mut i16;
        const FRAMES_PER_ENTRY: usize = ENTRY_BYTES / 4; // 1024 frames estéreo (4 B)
        const TOTAL_FRAMES: usize = FRAMES_PER_ENTRY * BDL_ENTRIES; // 16384 (≈341 ms)

        // Posição de leitura do hardware, em frames estéreo.
        let lpib = r32(bar, SD1_BASE + SDX_LPIB) as usize;
        let rd = (lpib / 4) % TOTAL_FRAMES;
        let mut pos = HDA_SD1_WPI.load(Ordering::Acquire) as usize % TOTAL_FRAMES;
        let used = (pos + TOTAL_FRAMES - rd) % TOTAL_FRAMES;
        let free_frames = TOTAL_FRAMES - 1 - used;

        let need = samples.len() * VOICE_DECIM;
        let to_write = need.min(free_frames);
        if to_write < need {
            // Back-pressure honesto: descarta o excedente em vez de rasgar o anel.
            PLAY_SAMPLES_DROPPED
                .fetch_add(((need - to_write) / VOICE_DECIM) as u64, Ordering::Relaxed);
        }

        let mut frames = 0usize;
        'outer: for &s in samples {
            for _ in 0..VOICE_DECIM {
                if frames >= to_write {
                    break 'outer;
                }
                let idx = pos * 2;
                core::ptr::write_volatile(virt.add(idx), s);
                core::ptr::write_volatile(virt.add(idx + 1), s);
                pos = (pos + 1) % TOTAL_FRAMES;
                frames += 1;
            }
        }
        HDA_SD1_WPI.store(pos as u32, Ordering::Release);
    }
}