//! KernelImage — blob nativo parseado (Wave 1). QMD/Walker/DISPATCH consomem isto;
//! nunca hardcodar `param_base=0x140` / `register_count=16` sem ler o blob.

use alloc::vec::Vec;
use crate::gpu::compute_abi::IsaTag;
use crate::gpu::kernel_pack::{IrOrigin, KernelPack};

#[derive(Debug, Clone)]
pub struct KernelImage {
    pub isa: IsaTag,
    pub code: Vec<u8>,
    pub regs: u32,
    pub shared: u32,
    pub barriers: u32,
    /// Lido do blob (CUBIN/HSACO/zebin); 0 se stub/desconhecido.
    pub param_base: u32,
    pub param_size: u32,
    pub ir: IrOrigin,
    pub is_stub: bool,
}

impl KernelImage {
    pub fn cpu_stub(isa: IsaTag, label: &[u8]) -> Self {
        let mut code = b"CPU_VECTOR_ADD_STUB\0".to_vec();
        code.extend_from_slice(label);
        Self {
            isa,
            code,
            regs: 16,
            shared: 0,
            barriers: 1,
            param_base: 0,
            param_size: 256,
            ir: IrOrigin::CpuStub,
            is_stub: true,
        }
    }
}

fn is_cpu_stub_bytes(payload: &[u8]) -> bool {
    payload.starts_with(b"CPU_VECTOR_ADD_STUB") || payload.starts_with(b"CPU_W2A8_STUB")
}

/// Parse payload cru (canário / dispatch) sem envelope NKP1.
pub fn from_blob(isa: IsaTag, prefer: IrOrigin, blob: &[u8]) -> KernelImage {
    if blob.is_empty() || is_cpu_stub_bytes(blob) {
        return KernelImage::cpu_stub(isa, blob.get(..20).unwrap_or(b"empty"));
    }
    let parsed = match prefer {
        IrOrigin::Cubin => crate::gpu::blob_cubin::parse(blob, isa),
        IrOrigin::Hsaco => crate::gpu::blob_hsaco::parse(blob, isa),
        IrOrigin::Zebin => crate::gpu::blob_zebin::parse(blob, isa),
        IrOrigin::CpuStub => None,
    };
    parsed.unwrap_or_else(|| {
        // ELF genérico: tentar cubin → hsaco → zebin
        crate::gpu::blob_cubin::parse(blob, isa)
            .or_else(|| crate::gpu::blob_hsaco::parse(blob, isa))
            .or_else(|| crate::gpu::blob_zebin::parse(blob, isa))
            .unwrap_or_else(|| KernelImage::cpu_stub(isa, b"unparsed"))
    })
}

/// Converte KernelPack verificado em KernelImage (parser por IrOrigin).
pub fn from_pack(pack: &KernelPack) -> Option<KernelImage> {
    let isa = pack.header.isa;
    let payload = pack.payload.as_slice();
    if is_cpu_stub_bytes(payload) || pack.header.ir == IrOrigin::CpuStub {
        return Some(KernelImage::cpu_stub(isa, payload.get(..20).unwrap_or(b"stub")));
    }
    Some(from_blob(isa, pack.header.ir, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_image_flags() {
        let img = KernelImage::cpu_stub(IsaTag::Sm61, b"sm_61");
        assert!(img.is_stub);
        assert_eq!(img.regs, 16);
        assert_eq!(img.param_base, 0);
    }
}
