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
        k_nano::slog_hal!("NKP", "info", "magic mismatch");
        return None;
    }
    let abi = read_u32(buf, 4)?;
    if abi != NKP_ABI {
        k_nano::slog_hal!("NKP", "info", "abi {} != {}", abi, NKP_ABI);
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
        k_nano::slog_hal!("NKP", "info", "content_hash mismatch expect={:#x} got={:#x}", expect, got);
        return None;
    }

    let verified = k_nano::identity::verify_trusted(canonical, &signature);
    if !verified {
        k_nano::slog_hal!("NKP", "info", "signature NOT trusted — pack em Escalate/deny ativo");
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

/// Carrega NKP do FAT (`NKP_*.BIN`). Unsigned ≠ ativo até promote_with_session.
/// k-hal: só FAT (sem hermes VFS — evita ciclo de deps).
pub fn load_named(name: &str) -> Option<KernelPack> {
    let aliases: &[&str] = match name {
        "NKP_GEN9.BIN" | "NKP_GEN9_BIN" => &["NKP_GEN9.BIN", "NKPGEN9.BIN", "NKP_GEN9_BIN"],
        "NKP_DG2.BIN" | "NKP_DG2_BIN" => &["NKP_DG2.BIN", "NKP_DG2_BIN"],
        "NKP_SM61.BIN" | "NKP_SM61_BIN" => &["NKP_SM61.BIN", "NKPSM61.BIN", "NKP_SM61_BIN"],
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

fn read_fat32_root(name: &str) -> Option<alloc::vec::Vec<u8>> {
    unsafe {
        let ata = k_nano::ATA_DRIVER.lock();
        let ata = ata.as_ref()?;
        let parts = k_nano::fat32::read_mbr(ata);
        for p in &parts {
            if !matches!(p.type_code, 0x0B | 0x0C | 0x1C | 0x73) {
                continue;
            }
            if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                if let Some(data) = fs.read_file(name) {
                    return Some(data);
                }
            }
        }
    }
    None
}

/// Match pack para vendor+isa+op.
/// Aceita signature trusted **ou** promove unsigned (hash ok) com session key.
pub fn find_active_pack(vendor: GpuVendor, isa: IsaTag, op: PackOp) -> Option<KernelPack> {
    let pv = PackVendor::from_gpu(vendor)?;
    let names = [
        "NKP_SM61.BIN",
        "NKP_SM61_BIN",
        "NKP_GEN9.BIN",
        "NKP_GEN9_BIN",
        "NKP_DG2.BIN",
        "NKP_DG2_BIN",
        "NKP_VECTOR_ADD.BIN",
        "NKP_VECTOR_ADD_BIN",
        "NKP_VECADD.BIN",
        "NKP_VECADD_BIN",
        "NKP_GFX90C.BIN",
        "NKP_GFX90C_BIN",
    ];
    for n in &names {
        if let Some(mut pack) = load_named(n) {
            if pack.header.vendor != pv || pack.header.isa != isa || pack.header.op != op {
                continue;
            }
            if !pack.verified {
                if let Some(p2) = promote_with_session(&pack) {
                    k_nano::slog_hal!("NKP", "info", "session-promoted {} isa={} bytes={}",
                        n,
                        isa.as_str(),
                        p2.payload.len());
                    pack = p2;
                } else {
                    k_nano::slog_hal!("NKP", "info", "{} hash ok but unsigned/session unavailable — skip Ready", n);
                    continue;
                }
            }
            if pack.verified {
                k_nano::slog_hal!("NKP", "info", "active pack {} isa={} bytes={}",
                    n,
                    isa.as_str(),
                    pack.payload.len());
                return Some(pack);
            }
        }
    }
    None
}

/// Reassina pack com session Ed25519 (boot). Hash deve já bater.
pub fn promote_with_session(pack: &KernelPack) -> Option<KernelPack> {
    if pack.verified {
        return Some(pack.clone());
    }
    if !k_nano::identity::session_ready() {
        return None;
    }
    let canonical = build_canonical(
        pack.header.vendor,
        pack.header.isa,
        pack.header.op,
        pack.header.golden,
        pack.header.compiler,
        pack.header.ir,
        pack.header.workgroup_x,
        pack.header.shared_mem,
        &pack.payload,
    );
    let expect = fnv1a64(&canonical);
    let got = u64::from_le_bytes(pack.content_hash);
    if expect != got {
        return None;
    }
    let sig = k_nano::identity::sign_session(&canonical)?;
    if !k_nano::identity::verify_trusted(&canonical, &sig) {
        return None;
    }
    Some(KernelPack {
        header: pack.header.clone(),
        payload: pack.payload.clone(),
        content_hash: pack.content_hash,
        signature: sig,
        verified: true,
    })
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
