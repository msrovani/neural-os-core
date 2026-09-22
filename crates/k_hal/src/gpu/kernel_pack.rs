//! KernelPack envelope — blob nativo assinado (ADR-0048–50 / ADR-0052-like).
//! magia | abi | vendor | isa | op | golden | compiler | ir | wg | smem | payload | hash | sig

use alloc::vec::Vec;
use crate::gpu::compute_abi::{GoldenId, IsaTag};
use crate::gpu::detect::GpuVendor;

pub const NKP_MAGIC: &[u8; 4] = b"NKP1";
pub const NKP_ABI: u32 = 1;
pub const NKP_HASH_LEN: usize = 8;
pub const NKP_SIG_LEN: usize = 64;
pub const NKP_HEADER_LEN: usize = 48; // até payload_len

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PackVendor {
    Nvidia = 1,
    Amd = 2,
    Intel = 3,
}

impl PackVendor {
    pub fn from_gpu(v: GpuVendor) -> Option<Self> {
        match v {
            GpuVendor::Nvidia => Some(PackVendor::Nvidia),
            GpuVendor::Amd => Some(PackVendor::Amd),
            GpuVendor::Intel => Some(PackVendor::Intel),
            _ => None,
        }
    }

    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            1 => Some(PackVendor::Nvidia),
            2 => Some(PackVendor::Amd),
            3 => Some(PackVendor::Intel),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PackOp {
    VectorAdd = 1,
    BitLinearW2A8 = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CompilerId {
    Cuda129 = 1,
    ClangAmdgcn = 2,
    RustcAmdgcn = 3,
    OclocIgc = 4,
    HostCpuLogic = 5,
    /// Rust-CUDA rustc_codegen_nvvm (host tool).
    RustCudaNvvm = 6,
    /// rustc stock nvptx64 + llvm-bitcode-linker.
    RustcNvptx = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IrOrigin {
    Cubin = 1,
    Hsaco = 2,
    Zebin = 3,
    CpuStub = 4,
}

#[derive(Debug, Clone)]
pub struct KernelPackHeader {
    pub abi: u32,
    pub vendor: PackVendor,
    pub isa: IsaTag,
    pub op: PackOp,
    pub golden: GoldenId,
    pub compiler: CompilerId,
    pub ir: IrOrigin,
    pub workgroup_x: u32,
    pub shared_mem: u32,
    pub payload_len: u32,
}

#[derive(Debug, Clone)]
pub struct KernelPack {
    pub header: KernelPackHeader,
    pub payload: Vec<u8>,
    pub content_hash: [u8; NKP_HASH_LEN],
    pub signature: [u8; NKP_SIG_LEN],
    pub verified: bool,
}

/// FNV-1a 64 do corpo canônico (header fields + payload, sem hash/sig).
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn isa_from_u32(v: u32) -> Option<IsaTag> {
    match v {
        0 => Some(IsaTag::None),
        1 => Some(IsaTag::Sm61),
        2 => Some(IsaTag::Sm75),
        3 => Some(IsaTag::Sm89),
        4 => Some(IsaTag::Gfx90c),
        5 => Some(IsaTag::Gfx1036),
        6 => Some(IsaTag::Gfx1103),
        7 => Some(IsaTag::Gfx1030),
        8 => Some(IsaTag::Gen9),
        9 => Some(IsaTag::Dg2),
        10 => Some(IsaTag::Sm52),
        11 => Some(IsaTag::Sm70),
        12 => Some(IsaTag::Sm80),
        13 => Some(IsaTag::Sm86),
        _ => None,
    }
}

fn read_u32(buf: &[u8], off: usize) -> Option<u32> {
    let s = buf.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Parse + verify hash; signature via identity se disponível.
pub fn parse_and_verify(buf: &[u8]) -> Option<KernelPack> {
    if buf.len() < NKP_HEADER_LEN + NKP_HASH_LEN + NKP_SIG_LEN {
        return None;
    }
    if &buf[0..4] != NKP_MAGIC {
        k_nano::slog_hal!("NKP", "fail", "magic mismatch");
        return None;
    }
    let abi = read_u32(buf, 4)?;
    if abi != NKP_ABI {
        k_nano::slog_hal!("NKP", "fail", "abi {} != {}", abi, NKP_ABI);
        return None;
    }
    let vendor = PackVendor::from_u32(read_u32(buf, 8)?)?;
    let isa = isa_from_u32(read_u32(buf, 12)?)?;
    let op = match read_u32(buf, 16)? {
        1 => PackOp::VectorAdd,
        2 => PackOp::BitLinearW2A8,
        _ => return None,
    };
    let golden = match read_u32(buf, 20)? {
        1 => GoldenId::VectorAdd,
        2 => GoldenId::BitLinearW2A8,
        _ => return None,
    };
    let compiler = match read_u32(buf, 24)? {
        1 => CompilerId::Cuda129,
        2 => CompilerId::ClangAmdgcn,
        3 => CompilerId::RustcAmdgcn,
        4 => CompilerId::OclocIgc,
        5 => CompilerId::HostCpuLogic,
        6 => CompilerId::RustCudaNvvm,
        7 => CompilerId::RustcNvptx,
        _ => return None,
    };
    let ir = match read_u32(buf, 28)? {
        1 => IrOrigin::Cubin,
        2 => IrOrigin::Hsaco,
        3 => IrOrigin::Zebin,
        4 => IrOrigin::CpuStub,
        _ => return None,
    };
    let workgroup_x = read_u32(buf, 32)?;
    let shared_mem = read_u32(buf, 36)?;
    let payload_len = read_u32(buf, 40)? as usize;
    let payload_end = NKP_HEADER_LEN + payload_len;
    if buf.len() < payload_end + NKP_HASH_LEN + NKP_SIG_LEN {
        return None;
    }
    let payload = buf[NKP_HEADER_LEN..payload_end].to_vec();
    let mut content_hash = [0u8; NKP_HASH_LEN];
    content_hash.copy_from_slice(&buf[payload_end..payload_end + NKP_HASH_LEN]);
    let mut signature = [0u8; NKP_SIG_LEN];
    signature.copy_from_slice(
        &buf[payload_end + NKP_HASH_LEN..payload_end + NKP_HASH_LEN + NKP_SIG_LEN],
    );

    let canonical = &buf[..payload_end];
    let expect = fnv1a64(canonical);
    let got = u64::from_le_bytes(content_hash);
    if expect != got {
        k_nano::slog_hal!("NKP", "fail", "content_hash mismatch expect={:#x} got={:#x}", expect, got);
        return None;
    }

    // H4 (onda1): pack só-PINNED. `verify_trusted` aceitaria a session PK,
    // que não prova origem externa — sessão é só audit local.
    let verified = k_nano::identity::verify_pinned(canonical, &signature);
    if !verified {
        k_nano::slog_hal!("NKP", "warn", "signature NOT trusted — pack em Escalate/deny ativo");
    }

    Some(KernelPack {
        header: KernelPackHeader {
            abi,
            vendor,
            isa,
            op,
            golden,
            compiler,
            ir,
            workgroup_x,
            shared_mem,
            payload_len: payload_len as u32,
        },
        payload,
        content_hash,
        signature,
        verified,
    })
}

/// Carrega NKP do FAT (`NKP_*.BIN`). Unsigned ≠ ativo — pinned-only (H4).
/// k-hal: só FAT (sem hermes VFS — evita ciclo de deps).
pub fn load_named(name: &str) -> Option<KernelPack> {
    let aliases: &[&str] = match name {
        "NKP_GEN9.BIN" | "NKP_GEN9_BIN" => &["NKP_GEN9.BIN", "NKPGEN9.BIN", "NKP_GEN9_BIN"],
        "NKP_DG2.BIN" | "NKP_DG2_BIN" => &["NKP_DG2.BIN", "NKP_DG2_BIN"],
        "NKP_SM61.BIN" | "NKP_SM61_BIN" => &["NKP_SM61.BIN", "NKPSM61.BIN", "NKP_SM61_BIN"],
        "NKP_SM75.BIN" => &["NKP_SM75.BIN", "NKPSM75.BIN"],
        "NKP_SM80.BIN" => &["NKP_SM80.BIN", "NKPSM80.BIN"],
        "NKP_SM86.BIN" => &["NKP_SM86.BIN", "NKPSM86.BIN"],
        "NKP_SM89.BIN" => &["NKP_SM89.BIN", "NKPSM89.BIN"],
        "NKP_W2A8_SM61.BIN" => &["NKP_W2A8_SM61.BIN", "NKPW2A861.BIN"],
        "NKP_W2A8_SM75.BIN" => &["NKP_W2A8_SM75.BIN", "NKPW2A875.BIN"],
        "NKP_W2A8_SM80.BIN" => &["NKP_W2A8_SM80.BIN", "NKPW2A880.BIN"],
        "NKP_W2A8_SM86.BIN" => &["NKP_W2A8_SM86.BIN", "NKPW2A886.BIN"],
        "NKP_W2A8_SM89.BIN" => &["NKP_W2A8_SM89.BIN", "NKPW2A889.BIN"],
        "NKP_W2A8_GEN9.BIN" => &["NKP_W2A8_GEN9.BIN", "NKPW2A8G9.BIN"],
        "NKP_W2A8_DG2.BIN" => &["NKP_W2A8_DG2.BIN", "NKPW2A8DG.BIN"],
        "NKP_W2A8_GFX1030.BIN" => &["NKP_W2A8_GFX1030.BIN", "NKPW2A8G0.BIN"],
        "NKP_VECTOR_ADD.BIN" | "NKP_VECTOR_ADD_BIN" => {
            &["NKP_VADD.BIN", "NKPVADD.BIN", "NKP_VECTOR_ADD_BIN"]
        }
        other => {
            if let Some(data) = read_fat32_root(other) {
                return parse_and_verify(&data);
            }
            return None;
        }
    };
    for a in aliases {
        if let Some(data) = read_fat32_root(a) {
            if let Some(p) = parse_and_verify(&data) {
                return Some(p);
            }
        }
    }
    None
}

/// ADR-0105 B0: presença no FAT (mesmo unsigned/stub) — ≠ Ready.
pub fn pack_present_on_fat(isa: IsaTag, op: PackOp) -> bool {
    for n in pack_name_candidates(isa, op) {
        if load_named(n).is_some() {
            return true;
        }
    }
    false
}

/// M3 (onda3): raiz FAT32 em QUALQUER device (ATA→AHCI→NVMe→USB), não só ATA.
fn read_fat32_root(name: &str) -> Option<alloc::vec::Vec<u8>> {
    unsafe {
        macro_rules! try_dev {
            ($lock:expr) => {
                if let Some(ref mut d) = *$lock {
                    let dev: &mut dyn k_nano::block_dev::BlockDevice = d;
                    for p in k_nano::fat32::partitions_on_dev(dev) {
                        if let Some(data) = k_nano::fat32::read_root_file_dev(dev, &p, name) {
                            return Some(data);
                        }
                    }
                }
            };
        }
        try_dev!(k_nano::globals::ATA_DRIVER.lock());
        try_dev!(k_nano::globals::AHCI_DRIVER.lock());
        try_dev!(k_nano::disk_agent::nvme::NVME_DRIVER.lock());
        try_dev!(k_nano::globals::USB_MSC.lock());
    }
    None
}

/// Nomes FAT candidatos por IsaTag — sem privilegiar sm_61.
fn pack_name_candidates(isa: IsaTag, op: PackOp) -> &'static [&'static str] {
    match (isa, op) {
        (IsaTag::Sm61, PackOp::BitLinearW2A8) => &[
            "NKP_W2A8_SM61.BIN",
            "NKPW2A861.BIN",
            "NKP_SM61.BIN",
            "NKPSM61.BIN",
        ],
        (IsaTag::Sm61, _) => &["NKP_SM61.BIN", "NKPSM61.BIN", "NKP_SM61_BIN"],
        (IsaTag::Sm52, PackOp::BitLinearW2A8) => &["NKP_W2A8_SM52.BIN", "NKP_SM52.BIN"],
        (IsaTag::Sm52, _) => &["NKP_SM52.BIN", "NKPSM52.BIN"],
        (IsaTag::Sm70, PackOp::BitLinearW2A8) => &["NKP_W2A8_SM70.BIN", "NKP_SM70.BIN"],
        (IsaTag::Sm70, _) => &["NKP_SM70.BIN", "NKPSM70.BIN"],
        (IsaTag::Sm75, PackOp::BitLinearW2A8) => &[
            "NKP_W2A8_SM75.BIN",
            "NKPW2A875.BIN",
            "NKP_SM75.BIN",
        ],
        (IsaTag::Sm75, _) => &["NKP_SM75.BIN", "NKPSM75.BIN"],
        (IsaTag::Sm80, PackOp::BitLinearW2A8) => &[
            "NKP_W2A8_SM80.BIN",
            "NKPW2A880.BIN",
            "NKP_SM80.BIN",
        ],
        (IsaTag::Sm80, _) => &["NKP_SM80.BIN", "NKPSM80.BIN"],
        (IsaTag::Sm86, PackOp::BitLinearW2A8) => &[
            "NKP_W2A8_SM86.BIN",
            "NKPW2A886.BIN",
            "NKP_W2A8_SM80.BIN", // fallback Ampere baseline
            "NKP_SM86.BIN",
        ],
        (IsaTag::Sm86, _) => &["NKP_SM86.BIN", "NKPSM86.BIN", "NKP_SM80.BIN"],
        (IsaTag::Sm89, PackOp::BitLinearW2A8) => &[
            "NKP_W2A8_SM89.BIN",
            "NKPW2A889.BIN",
            "NKP_SM89.BIN",
        ],
        (IsaTag::Sm89, _) => &["NKP_SM89.BIN", "NKPSM89.BIN"],
        (IsaTag::Gen9, PackOp::BitLinearW2A8) => &[
            "NKP_W2A8_GEN9.BIN",
            "NKPW2A8G9.BIN",
            "NKP_GEN9.BIN",
            "NKPGEN9.BIN",
        ],
        (IsaTag::Gen9, _) => &["NKP_GEN9.BIN", "NKPGEN9.BIN", "NKP_GEN9_BIN"],
        (IsaTag::Dg2, PackOp::BitLinearW2A8) => &["NKP_W2A8_DG2.BIN", "NKP_DG2.BIN"],
        (IsaTag::Dg2, _) => &["NKP_DG2.BIN", "NKP_DG2_BIN"],
        (IsaTag::Gfx1030, PackOp::BitLinearW2A8) => &["NKP_W2A8_GFX1030.BIN", "NKP_GFX1030.BIN"],
        (IsaTag::Gfx1030, _) => &["NKP_GFX1030.BIN", "NKPGFX30.BIN"],
        (IsaTag::Gfx1036, _) => &["NKP_GFX1036.BIN", "NKPGFX36.BIN"],
        (IsaTag::Gfx1103, PackOp::BitLinearW2A8) => &["NKP_W2A8_GFX1103.BIN", "NKP_GFX1103.BIN"],
        (IsaTag::Gfx1103, _) => &["NKP_GFX1103.BIN", "NKPGFX03.BIN"],
        (IsaTag::Gfx90c, _) => &["NKP_GFX90C.BIN", "NKPGFX90.BIN", "NKP_GFX90C_BIN"],
        (IsaTag::None, _) => &[
            "NKP_VECTOR_ADD.BIN",
            "NKPVADD.BIN",
            "NKP_VECADD.BIN",
        ],
    }
}

/// Match pack para vendor+isa+op (multi-ISA; sem hardcodar só sm_61).
/// Só aceita assinatura PINNED (H4); unsigned + hash ok ≠ Ready.
pub fn find_active_pack(vendor: GpuVendor, isa: IsaTag, op: PackOp) -> Option<KernelPack> {
    let pv = PackVendor::from_gpu(vendor)?;
    let mut names: alloc::vec::Vec<&str> = pack_name_candidates(isa, op).to_vec();
    // Fallbacks genéricos (vector_add / legacy).
    names.extend_from_slice(&[
        "NKP_VECTOR_ADD.BIN",
        "NKP_VECTOR_ADD_BIN",
        "NKP_VECADD.BIN",
        "NKP_VECADD_BIN",
    ]);
    for n in &names {
        if let Some(pack) = load_named(n) {
            if pack.header.vendor != pv || pack.header.isa != isa || pack.header.op != op {
                continue;
            }
            if !pack.verified {
                k_nano::slog_hal!(
                    "NKP",
                    "warn",
                    "{} unsigned (pinned-only, H4) — skip Ready; assine com release key",
                    n
                );
                continue;
            }
            if pack.verified {
                k_nano::slog_hal!(
                    "NKP",
                    "ok",
                    "active pack {} isa={} op={} bytes={}",
                    n,
                    isa.as_str(),
                    op as u32,
                    pack.payload.len()
                );
                return Some(pack);
            }
        }
    }
    None
}

/// True se o pack é CpuStub (layout/golden only — não promove a device Ready).
pub fn is_cpu_stub_pack(pack: &KernelPack) -> bool {
    pack.header.ir == IrOrigin::CpuStub
}

/// H4 (onda1): a sessão NUNCA instala pack/skills — `promote_with_session`
/// virou refuse explícito. Um blob assinado pela session PK não prova origem
/// externa; instalação exige release key pinada (`verify_pinned` no parse).
/// Mantido como API negativa p/ chamadores legados: sempre `None`.
pub fn promote_with_session(pack: &KernelPack) -> Option<KernelPack> {
    k_nano::slog_hal!(
        "NKP",
        "warn",
        "promote refuse isa={} op={:?} — session PK não instala pack (H4); assine com release key",
        pack.header.isa.as_str(),
        pack.header.op
    );
    None
}

/// Serializa header+payload canônico (host tools espelham este layout).
pub fn build_canonical(
    vendor: PackVendor,
    isa: IsaTag,
    op: PackOp,
    golden: GoldenId,
    compiler: CompilerId,
    ir: IrOrigin,
    workgroup_x: u32,
    shared_mem: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(NKP_HEADER_LEN + payload.len());
    out.extend_from_slice(NKP_MAGIC);
    out.extend_from_slice(&NKP_ABI.to_le_bytes());
    out.extend_from_slice(&(vendor as u32).to_le_bytes());
    out.extend_from_slice(&(isa as u32).to_le_bytes());
    out.extend_from_slice(&(op as u32).to_le_bytes());
    out.extend_from_slice(&(golden as u32).to_le_bytes());
    out.extend_from_slice(&(compiler as u32).to_le_bytes());
    out.extend_from_slice(&(ir as u32).to_le_bytes());
    out.extend_from_slice(&workgroup_x.to_le_bytes());
    out.extend_from_slice(&shared_mem.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    // pad to 48
    while out.len() < NKP_HEADER_LEN {
        out.push(0);
    }
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::compute_abi::IsaTag;

    #[test]
    fn cpu_stub_pack_refuses_promote() {
        let pack = KernelPack {
            header: KernelPackHeader {
                abi: NKP_ABI,
                vendor: PackVendor::Nvidia,
                isa: IsaTag::Sm61,
                op: PackOp::BitLinearW2A8,
                golden: GoldenId::VectorAdd,
                compiler: CompilerId::HostCpuLogic,
                ir: IrOrigin::CpuStub,
                workgroup_x: 1,
                shared_mem: 0,
                payload_len: 0,
            },
            payload: Vec::new(),
            content_hash: [0u8; NKP_HASH_LEN],
            signature: [0u8; NKP_SIG_LEN],
            verified: false,
        };
        assert!(is_cpu_stub_pack(&pack));
        assert!(promote_with_session(&pack).is_none());
    }
}
