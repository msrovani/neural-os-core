//! PlatformProbe — HypervisorKind + CpuFeatures + CacheTopology + FeatureGate (ADR-0055).
//! Fonte única de verdade cedo no boot; ISA ∩ política sandbox.

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

/// Ambiente de virtualização detectado via CPUID 0x40000000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HypervisorKind {
    None = 0,
    Kvm = 1,
    Tcg = 2,
    MicrosoftHv = 3,
    VBox = 4,
    VMware = 5,
    QemuGeneric = 6,
    UnknownHv = 7,
}

impl HypervisorKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "baremetal",
            Self::Kvm => "KVM",
            Self::Tcg => "TCG",
            Self::MicrosoftHv => "WHPX",
            Self::VBox => "VBox",
            Self::VMware => "VMware",
            Self::QemuGeneric => "QEMU",
            Self::UnknownHv => "UnknownHv",
        }
    }

    pub fn is_sandbox(self) -> bool {
        self != Self::None
    }
}

/// Sinalizadores de instrução (CPUID leaf 1 + 7).
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuFeatures {
    pub sse42: bool,
    pub popcnt: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fma: bool,
    pub bmi1: bool,
    pub bmi2: bool,
    /// CR4.OSXSAVE já ligado (CPUID.1:ECX[27]) — pós enable_simd.
    pub osxsave: bool,
    /// CPU oferece XSAVE (CPUID.1:ECX[26]) — capacidade ISA pré-enable.
    pub xsave: bool,
    pub rdrand: bool,
    pub clflushopt: bool,
    pub clwb: bool,
    pub waitpkg: bool,
    pub avx512f: bool,
    pub amx_tile: bool,
    pub hybrid: bool,
}

/// Path de dispatch ISA escolhido no boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IsaPath {
    Scalar = 0,
    Sse42 = 1,
    Avx2Fma = 2,
    Avx512F = 3,
}

/// Topologia de cache (bytes).
#[derive(Debug, Clone, Copy)]
pub struct CacheTopology {
    pub l1d: usize,
    pub l1i: usize,
    pub l2: usize,
    pub l3: usize,
    pub line_size: usize,
    pub clflush_line_size: usize,
}

impl Default for CacheTopology {
    fn default() -> Self {
        Self {
            l1d: 32768,
            l1i: 32768,
            l2: 262144,
            l3: 4194304,
            line_size: 64,
            clflush_line_size: 64,
        }
    }
}

/// FeatureGate atômico — bits de política efetiva.
#[derive(Debug, Clone, Copy)]
pub struct FeatureGate {
    pub allow_smp: bool,
    pub allow_avx2: bool,
    pub allow_avx512: bool,
    pub allow_fma: bool,
    pub allow_ipi_broadcast: bool,
    pub prefer_serial_tunnel: bool,
    pub max_aps: u8,
    pub allow_ep_core_detect: bool,
    pub allow_prefetch: bool,
    pub allow_clflushopt: bool,
    pub isa_path: IsaPath,
}

impl FeatureGate {
    pub const fn disabled() -> Self {
        Self {
            allow_smp: false,
            allow_avx2: false,
            allow_avx512: false,
            allow_fma: false,
            allow_ipi_broadcast: false,
            prefer_serial_tunnel: false,
            max_aps: 0,
            allow_ep_core_detect: false,
            allow_prefetch: false,
            allow_clflushopt: false,
            isa_path: IsaPath::Scalar,
        }
    }
}

// ─── Globals ───────────────────────────────────────────────────────────

static HV_KIND: AtomicU8 = AtomicU8::new(0);
static GATE_BITS: AtomicU32 = AtomicU32::new(0);
static MAX_APS: AtomicU8 = AtomicU8::new(0);
static ISA_PATH: AtomicU8 = AtomicU8::new(0);
static CACHE_PACKED: AtomicU64 = AtomicU64::new(0); // line | l1d>>10 | l2>>10 | l3>>20 (approx log)
static PROBED: AtomicU8 = AtomicU8::new(0);

const BIT_SMP: u32 = 1 << 0;
const BIT_AVX2: u32 = 1 << 1;
const BIT_AVX512: u32 = 1 << 2;
const BIT_FMA: u32 = 1 << 3;
const BIT_IPI: u32 = 1 << 4;
const BIT_SERIAL: u32 = 1 << 5;
const BIT_EP: u32 = 1 << 6;
const BIT_PREFETCH: u32 = 1 << 7;
const BIT_CLFLUSHOPT: u32 = 1 << 8;

static mut CPU_FEATURES: CpuFeatures = CpuFeatures {
    sse42: false,
    popcnt: false,
    avx: false,
    avx2: false,
    fma: false,
    bmi1: false,
    bmi2: false,
    osxsave: false,
    xsave: false,
    rdrand: false,
    clflushopt: false,
    clwb: false,
    waitpkg: false,
    avx512f: false,
    amx_tile: false,
    hybrid: false,
};

static mut CACHE_TOPO: CacheTopology = CacheTopology {
    l1d: 32768,
    l1i: 32768,
    l2: 262144,
    l3: 4194304,
    line_size: 64,
    clflush_line_size: 64,
};

fn hv_from_u8(v: u8) -> HypervisorKind {
    match v {
        1 => HypervisorKind::Kvm,
        2 => HypervisorKind::Tcg,
        3 => HypervisorKind::MicrosoftHv,
        4 => HypervisorKind::VBox,
        5 => HypervisorKind::VMware,
        6 => HypervisorKind::QemuGeneric,
        7 => HypervisorKind::UnknownHv,
        _ => HypervisorKind::None,
    }
}

/// Detecta hypervisor via CPUID leaf1.ECX.31 + leaf 0x40000000 vendor.
pub fn detect_hypervisor() -> HypervisorKind {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf1 = core::arch::x86_64::__cpuid(1);
        if (leaf1.ecx & (1 << 31)) == 0 {
            return HypervisorKind::None;
        }
        let hv = core::arch::x86_64::__cpuid(0x40000000);
        let mut name = [0u8; 12];
        name[0..4].copy_from_slice(&hv.ebx.to_le_bytes());
        name[4..8].copy_from_slice(&hv.ecx.to_le_bytes());
        name[8..12].copy_from_slice(&hv.edx.to_le_bytes());
        // Compare ASCII case-insensitive without alloc
        let has = |pat: &[u8]| {
            name.windows(pat.len()).any(|w| {
                w.iter()
                    .zip(pat.iter())
                    .all(|(a, b)| a.to_ascii_lowercase() == *b)
            })
        };
        if has(b"tcg") {
            return HypervisorKind::Tcg;
        }
        if has(b"kvm") {
            return HypervisorKind::Kvm;
        }
        if &name == b"Microsoft Hv" {
            return HypervisorKind::MicrosoftHv;
        }
        if has(b"vbox") {
            return HypervisorKind::VBox;
        }
        if has(b"vmware") {
            return HypervisorKind::VMware;
        }
        if has(b"qemu") {
            return HypervisorKind::QemuGeneric;
        }
        HypervisorKind::UnknownHv
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        HypervisorKind::None
    }
}

pub fn detect_cpu_features() -> CpuFeatures {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let l1 = core::arch::x86_64::__cpuid(1);
        let l7 = core::arch::x86_64::__cpuid_count(7, 0);
        let max_leaf = core::arch::x86_64::__cpuid(0).eax;
        let hybrid = if max_leaf >= 0x1A {
            let h = core::arch::x86_64::__cpuid(0x1A);
            h.eax != 0
        } else {
            false
        };
        CpuFeatures {
            sse42: (l1.ecx & (1 << 20)) != 0,
            popcnt: (l1.ecx & (1 << 23)) != 0,
            avx: (l1.ecx & (1 << 28)) != 0,
            osxsave: (l1.ecx & (1 << 27)) != 0,
            xsave: (l1.ecx & (1 << 26)) != 0,
            fma: (l1.ecx & (1 << 12)) != 0,
            rdrand: (l1.ecx & (1 << 30)) != 0,
            avx2: (l7.ebx & (1 << 5)) != 0,
            bmi1: (l7.ebx & (1 << 3)) != 0,
            bmi2: (l7.ebx & (1 << 8)) != 0,
            clflushopt: (l7.ebx & (1 << 23)) != 0,
            clwb: (l7.ebx & (1 << 24)) != 0,
            avx512f: (l7.ebx & (1 << 16)) != 0,
            amx_tile: (l7.edx & (1 << 24)) != 0,
            waitpkg: (l7.ecx & (1 << 5)) != 0,
            hybrid,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        CpuFeatures::default()
    }
}

fn cache_level_bytes(subleaf: u32) -> Option<(u8, usize, usize)> {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let info = core::arch::x86_64::__cpuid_count(0x04, subleaf);
        let ctype = (info.eax & 0x1F) as u8;
        if ctype == 0 {
            // AMD
            let amd = core::arch::x86_64::__cpuid_count(0x8000_001D, subleaf);
            let ctype = (amd.eax & 0x1F) as u8;
            if ctype == 0 {
                return None;
            }
            let level = ((amd.eax >> 5) & 0x7) as u8;
            let ways = ((amd.ebx >> 22) & 0x3FF) + 1;
            let parts = ((amd.ebx >> 12) & 0x3FF) + 1;
            let line = (amd.ebx & 0xFFF) + 1;
            let sets = amd.ecx + 1;
            let size = (ways * parts * line * sets) as usize;
            return Some((level, size, line as usize));
        }
        let level = ((info.eax >> 5) & 0x7) as u8;
        let ways = ((info.ebx >> 22) & 0x3FF) + 1;
        let parts = ((info.ebx >> 12) & 0x3FF) + 1;
        let line = (info.ebx & 0xFFF) + 1;
        let sets = info.ecx + 1;
        let size = (ways * parts * line * sets) as usize;
        Some((level, size, line as usize))
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = subleaf;
        None
    }
}

pub fn detect_cache_topology() -> CacheTopology {
    let mut topo = CacheTopology::default();
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let l1 = core::arch::x86_64::__cpuid(1);
        let clf = ((l1.ebx >> 8) & 0xFF) as usize * 8;
        if clf >= 32 && clf <= 128 {
            topo.clflush_line_size = clf;
            topo.line_size = clf;
        }
        for sub in 0..8u32 {
            if let Some((level, size, line)) = cache_level_bytes(sub) {
                if line >= 32 && line <= 128 {
                    topo.line_size = line;
                }
                // Intel: level 1 + type data(1)/unified(3) ≈ L1D; type instr(2) ≈ L1I
                // Simplificação: primeiro size em level 1 → l1d; level 2 → l2; level 3 → l3
                match level {
                    1 => {
                        if topo.l1d == 32768 || size > topo.l1d {
                            // first L1 entry often data; keep max
                            if topo.l1d == CacheTopology::default().l1d {
                                topo.l1d = size;
                            } else if size != topo.l1d {
                                topo.l1i = size;
                            }
                        }
                    }
                    2 => topo.l2 = size,
                    3 => topo.l3 = size,
                    _ => {}
                }
            } else {
                break;
            }
        }
    }
    topo
}

/// Política ADR-0055: ISA ∩ hypervisor.
pub fn build_gate(hv: HypervisorKind, isa: &CpuFeatures) -> FeatureGate {
    let (allow_smp, max_aps, prefer_serial) = match hv {
        HypervisorKind::None => (true, 255u8, false),
        HypervisorKind::Kvm => (true, 4, true),
        HypervisorKind::Tcg => (true, 4, true),
        HypervisorKind::MicrosoftHv => (false, 0, true),
        HypervisorKind::VBox | HypervisorKind::VMware => (false, 0, true),
        HypervisorKind::QemuGeneric | HypervisorKind::UnknownHv => (true, 4, true),
    };

    let tcg = hv == HypervisorKind::Tcg;
    // Gate usa XSAVE (capacidade), não OSXSAVE (já habilitado) — senão AVX2 nunca liga pré-CR4.
    let allow_avx2 = isa.avx2 && isa.xsave && isa.avx && !tcg;
    let allow_avx512 = isa.avx512f && isa.xsave && isa.avx && !tcg;
    let allow_fma = allow_avx2 && isa.fma;

    let isa_path = if allow_avx512 {
        IsaPath::Avx512F
    } else if allow_avx2 {
        IsaPath::Avx2Fma
    } else if isa.sse42 {
        IsaPath::Sse42
    } else {
        IsaPath::Scalar
    };

    FeatureGate {
        allow_smp,
        allow_avx2,
        allow_avx512,
        allow_fma,
        allow_ipi_broadcast: allow_smp,
        prefer_serial_tunnel: prefer_serial,
        max_aps,
        allow_ep_core_detect: hv == HypervisorKind::None || hv == HypervisorKind::Kvm,
        allow_prefetch: true,
        allow_clflushopt: isa.clflushopt,
        isa_path,
    }
}

/// Probe completo — chamar uma vez cedo no boot (antes de enable_simd / SMP).
pub fn detect() {
    let hv = detect_hypervisor();
    let isa = detect_cpu_features();
    let cache = detect_cache_topology();
    let gate = build_gate(hv, &isa);

    HV_KIND.store(hv as u8, Ordering::Release);

    let mut bits = 0u32;
    if gate.allow_smp {
        bits |= BIT_SMP;
    }
    if gate.allow_avx2 {
        bits |= BIT_AVX2;
    }
    if gate.allow_avx512 {
        bits |= BIT_AVX512;
    }
    if gate.allow_fma {
        bits |= BIT_FMA;
    }
    if gate.allow_ipi_broadcast {
        bits |= BIT_IPI;
    }
    if gate.prefer_serial_tunnel {
        bits |= BIT_SERIAL;
    }
    if gate.allow_ep_core_detect {
        bits |= BIT_EP;
    }
    if gate.allow_prefetch {
        bits |= BIT_PREFETCH;
    }
    if gate.allow_clflushopt {
        bits |= BIT_CLFLUSHOPT;
    }
    GATE_BITS.store(bits, Ordering::Release);
    MAX_APS.store(gate.max_aps, Ordering::Release);
    ISA_PATH.store(gate.isa_path as u8, Ordering::Release);

    unsafe {
        CPU_FEATURES = isa;
        CACHE_TOPO = cache;
    }

    // Sync SystemEnv coarse
    match hv {
        HypervisorKind::None => crate::env::set(crate::env::SystemEnv::HwReal),
        HypervisorKind::VBox => crate::env::set(crate::env::SystemEnv::VBoxSandbox),
        _ if hv.is_sandbox() => crate::env::set(crate::env::SystemEnv::QemuSandbox),
        _ => {}
    }

    PROBED.store(1, Ordering::Release);

    crate::slog_nano!(
        "ENV",
        "info",
        "hv={} sandbox={} smp={} avx2={} avx512={} fma={} max_aps={} isa={}",
        hv.name(),
        hv.is_sandbox(),
        gate.allow_smp,
        gate.allow_avx2,
        gate.allow_avx512,
        gate.allow_fma,
        gate.max_aps,
        match gate.isa_path {
            IsaPath::Scalar => "scalar",
            IsaPath::Sse42 => "sse42",
            IsaPath::Avx2Fma => "avx2+fma",
            IsaPath::Avx512F => "avx512f",
        }
    );
    crate::slog_nano!(
        "CPU",
        "info",
        "sse42={} avx={} avx2={} fma={} bmi2={} clflushopt={} amx={} hybrid={}",
        isa.sse42,
        isa.avx,
        isa.avx2,
        isa.fma,
        isa.bmi2,
        isa.clflushopt,
        isa.amx_tile,
        isa.hybrid
    );
    crate::slog_nano!(
        "CACHE",
        "info",
        "L1D={} L1I={} L2={} L3={} line={}",
        cache.l1d,
        cache.l1i,
        cache.l2,
        cache.l3,
        cache.line_size
    );

    let _ = CACHE_PACKED; // reserved
}

pub fn hypervisor() -> HypervisorKind {
    hv_from_u8(HV_KIND.load(Ordering::Acquire))
}

pub fn gate() -> FeatureGate {
    let bits = GATE_BITS.load(Ordering::Acquire);
    let isa_path = match ISA_PATH.load(Ordering::Acquire) {
        3 => IsaPath::Avx512F,
        2 => IsaPath::Avx2Fma,
        1 => IsaPath::Sse42,
        _ => IsaPath::Scalar,
    };
    FeatureGate {
        allow_smp: bits & BIT_SMP != 0,
        allow_avx2: bits & BIT_AVX2 != 0,
        allow_avx512: bits & BIT_AVX512 != 0,
        allow_fma: bits & BIT_FMA != 0,
        allow_ipi_broadcast: bits & BIT_IPI != 0,
        prefer_serial_tunnel: bits & BIT_SERIAL != 0,
        max_aps: MAX_APS.load(Ordering::Acquire),
        allow_ep_core_detect: bits & BIT_EP != 0,
        allow_prefetch: bits & BIT_PREFETCH != 0,
        allow_clflushopt: bits & BIT_CLFLUSHOPT != 0,
        isa_path,
    }
}

pub fn cpu_features() -> CpuFeatures {
    if PROBED.load(Ordering::Acquire) == 0 {
        return CpuFeatures::default();
    }
    unsafe { CPU_FEATURES }
}

pub fn cache_topology() -> CacheTopology {
    if PROBED.load(Ordering::Acquire) == 0 {
        return CacheTopology::default();
    }
    unsafe { CACHE_TOPO }
}

pub fn allow_avx2() -> bool {
    gate().allow_avx2
}

pub fn allow_avx512() -> bool {
    gate().allow_avx512
}

pub fn allow_smp() -> bool {
    gate().allow_smp
}

pub fn max_aps() -> u8 {
    gate().max_aps
}

pub fn isa_path() -> IsaPath {
    gate().isa_path
}

pub fn cache_size(level: usize) -> usize {
    let t = cache_topology();
    match level {
        0 => t.l1d,
        1 => t.l1i,
        2 => t.l2,
        3 => t.l3,
        _ => t.l1d,
    }
}

pub fn line_size() -> usize {
    cache_topology().line_size.max(64)
}

pub fn optimal_attention_block(hidden: usize) -> usize {
    let l1 = cache_size(0);
    let qk_bytes = hidden.saturating_mul(2).saturating_mul(4);
    let needed = qk_bytes.saturating_add(4).max(1);
    let block_by_l1 = l1 / needed;
    if block_by_l1 >= 4 {
        return block_by_l1.min(64);
    }
    let l2 = cache_size(2);
    (l2 / needed).min(128).max(4)
}

/// Tile rows para matmul paralelo baseado em L2 working set.
pub fn matmul_tile_rows(k: usize, n: usize) -> usize {
    let l2 = cache_size(2);
    // 3 panels f32: A row-block, B, C ≈ (tile*k + k*n/cores + tile*n) * 4
    let bytes_per_row = k.saturating_mul(4).saturating_add(n.saturating_mul(4)).max(64);
    let rows = (l2 / 4) / bytes_per_row;
    rows.clamp(4, 256)
}

/// Intel Thread Director / HFI — inventário only (Fase C). Retorna true se CPUID sugere ITD.
pub fn itd_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max = core::arch::x86_64::__cpuid(0).eax;
        if max < 6 {
            return false;
        }
        // Leaf 6 ECX bit 0 = Hardware Feedback Interface (aproximação ITD stack)
        let l6 = core::arch::x86_64::__cpuid(6);
        (l6.ecx & 1) != 0 && hypervisor() == HypervisorKind::None
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

pub fn log_itd_probe() {
    let ok = itd_available();
    crate::slog_nano!(
        "CPU",
        "info",
        "ITD/HFI probe={} (scheduling hints {})",
        ok,
        if ok { "available — deferred enable" } else { "absent" }
    );
}
