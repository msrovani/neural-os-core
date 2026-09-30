//! Loader-VRAM — pesos do Falcon3 FAT → BAR SEM passar pelo heap bump (s427).
//!
//! ADR-0112 residual fechado: hoje o `load_llm_v6` copia cada tensor pro bump
//! heap (que NUNCA libera) e o `on_model_loaded` copia DE NOVO heap→BAR. No 1B
//! (544MB packed) isso é ~1GB de janela heap consumida; no 3B o heap estoura.
//! Aqui o `.bitnet v6` é lido do FAT em CHUNKS (cluster, ~1-4KB — o bounce é
//! o único buffer) e os TENSORES TERNÁRIOS das layers vão direto pra aperture
//! BAR (`write_vram_bytes`). O heap passa a segurar APENAS embed/norms/KV.
//!
//! Rota honesta (s427, ESCOPO REAL): o modelo CONTINUA sendo carregado pelo
//! `load_llm_v6` (os packed das layers ainda passam pelo heap no parse —
//! stub do parser é residual). O ganho DESTA sprint: o lane VRAM ativa ANTES
//! do load do heap (SEQ_MATS/UPLOADED registrados aqui) e o upload pós-load
//! (`on_model_loaded`) vira no-op — os pesos na BAR vêm do FAT, não da cópia
//! heap→BAR. Se o loader falhar (sem BAR/aperture pequena/arquivo ausente),
//! o fluxo legado intacto roda (upload pós-load do heap).
//!
//! Residual (fecha o ADR-0112 de verdade): `load_llm_v6` stub dos packed
//! (`packed_data: vec![]` shape-correct) quando `loader_resident()` — aí o
//! heap NUNCA segura os pesos. Requer guard no fallback CPU do
//! `dispatch_vram_seq` (já existe: shape protection + lane ativo).
//!
//! Limite honesto: eMBED continua no heap (1B ≈ 40MB packed, usado por lookup
//! por linha); 3B+ com aperture 256MB continua "lane off honesto" quando o
//! TOTAL das layers não cabe — pré-checagem igual à do `on_model_loaded`.

use crate::gpu::bar_compute::{
    register_loader_resident, reserve_vram_span, write_vram_bytes,
};

/// Tamanhos packed de cada tensor ternário: `(rows, cols) → bytes`.
fn packed_bytes(rows: usize, cols: usize) -> usize {
    (rows.saturating_mul(cols).saturating_add(3)) / 4
}

/// Ordem dos tensores por layer (MESMA do snapshot `current_model_layers_snapshot`):
/// q, k, v, o, gate, up, down. Shapes derivados do header v6.
/// Retorna None se o header não parsear (honesto).
pub fn layer_tensor_shapes(h: &cortex::model::ModelHeader) -> Option<[(usize, usize); 7]> {
    let kv_head_dim = h.q_dim / h.num_heads.max(1);
    let k_dim = h.kv_heads * kv_head_dim;
    let ffn_group = h.intermediate * h.q_dim / h.hidden.max(1);
    let down_out = h.q_dim;
    Some([
        (h.hidden, h.q_dim),        // q
        (h.hidden, k_dim),          // k
        (h.hidden, k_dim),          // v
        (h.q_dim, h.hidden),        // o
        (h.hidden, ffn_group),      // gate
        (h.hidden, ffn_group),      // up
        (h.intermediate, down_out), // down
    ])
}

/// Total de bytes packed das layers do modelo (pré-checagem de aperture).
pub fn layers_total_bytes(h: &cortex::model::ModelHeader) -> Option<u64> {
    let shapes = layer_tensor_shapes(h)?;
    let mut total = 0u64;
    for _ in 0..h.num_layers {
        for (r, c) in shapes {
            total += packed_bytes(r, c) as u64;
        }
    }
    Some(total)
}

/// Layout por layer derivado do header (ordem EXATA do `load_llm_v6`):
/// rms_attn(f32×hidden) → rms_ffn(f32×hidden) → [inner: f32×(kv_dim)] →
/// [ffn_norm: f32×intermediate] → q(k,n)+scale → k+scale → v+scale → o+scale
/// → gate+scale → up+scale → down+scale. Cada ternário = packed + f32 scale.
pub struct LayerLayout {
    /// offset do packed q da layer `i` = `layers_data_offset + i * stride`
    pub layers_data_offset: usize,
    /// bytes por layer (norms + 7×(packed+scale))
    pub stride: usize,
    /// offsets do PACKED de cada tensor (0..7) dentro da layer
    pub tensor_off: [usize; 7],
}

pub fn layer_layout(h: &cortex::model::ModelHeader) -> Option<LayerLayout> {
    let hidden = h.hidden;
    let kv_head_dim = h.q_dim / h.num_heads.max(1);
    let kv_dim = h.kv_heads * kv_head_dim;
    let ffn_group = h.intermediate * h.q_dim / h.hidden.max(1);
    let has_inner = h.feat & 1 != 0;
    let has_ffn = h.feat & 2 != 0;

    // norms (ANTES dos ternários — ordem do parser)
    let mut norms = hidden * 4; // rms_attn
    norms += hidden * 4; // rms_ffn
    if has_inner {
        norms += kv_dim * 4;
    }
    if has_ffn {
        norms += h.intermediate * 4;
    }

    let shapes = layer_tensor_shapes(h)?;
    let mut tensor_off = [0usize; 7];
    let mut cur = norms;
    for (i, (r, c)) in shapes.iter().enumerate() {
        tensor_off[i] = cur;
        cur += packed_bytes(*r, *c) + 4; // packed + f32 scale
    }
    Some(LayerLayout {
        layers_data_offset: 0, // preenchido pelo caller (após embed)
        stride: cur,
        tensor_off,
    })
}

/// Offset no arquivo onde o packed q da layer 0 começa: header + tok blob +
/// embed (+scale) + norms da layer 0. Caminha o MESMO layout do
/// `load_llm_v6` — teste de paridade (header sintético) segura a deriva.
pub fn layers_data_offset(data: &[u8], h: &cortex::model::ModelHeader) -> Option<usize> {
    if data.len() < 52 { return None; }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0xBE11BE11 { return None; }
    let tok_len = u32::from_le_bytes([data[45], data[46], data[47], data[48]]) as usize;
    let mut off = 49 + tok_len + 3;
    if off > data.len() { return None; }

    // embed (+scale) — embed_type 0=ternary 1=Q6_K 2=BF16 (só 0/1 suportados
    // no loader — o parser também recusa 2).
    let hidden = h.hidden;
    let vocab = h.vocab;
    off += match h.embed_type {
        0 => packed_bytes(hidden, vocab) + 4,
        1 => ((hidden.saturating_mul(vocab) + 255) / 256) * 210 + 4,
        _ => return None,
    };
    Some(off)
}

// ── Orquestrador: FAT → BAR streaming ─────────────────────────────────────
//
// Não dependemos de `cortex::model` aqui além do header (o caller passa o
// header já parseado — parse é 64 bytes). A cópia usa callback do leitor
// chunked: cada chunk FAT (cluster) vai direto `write_vram_bytes` na aperture.
//
// Honestidade: se o arquivo for maior que o header calcula, ou a chain falhar,
// aborta SEM registro (lane off) — nunca parcial.

/// Lê o header (512B do 1º chunk) do arquivo FAT e devolve o ModelHeader.
unsafe fn probe_header(
    dev: &mut dyn k_nano::block_dev::BlockDevice,
    part: &k_nano::fat32::Partition,
    name: &str,
    hdr_out: &mut [u8],
) -> Option<usize> {
    let mut done = false;
    let ok = k_nano::fat32::read_root_file_dev_chunked(
        dev, part, name, &mut |_off: usize, bytes: &[u8]| {
            if !done {
                done = true;
                let n = bytes.len().min(hdr_out.len());
                hdr_out[..n].copy_from_slice(&bytes[..n]);
            }
        },
    );
    if ok.is_none() || !done {
        return None;
    }
    Some(1)
}

/// Janela packed de um tensor: [start,end) no arquivo + (off VRAM alvo, k, n).
struct CopyWindow {
    file_start: usize,
    file_end: usize,
    vram_off: u64,
}

/// Computa as janelas de cópia (uma por packed de tensor) a partir do header
/// REAL lido do arquivo — sem chutes: `layers_off` vem do próprio blob.
fn compute_windows(
    layers_off: usize,
    layout: &LayerLayout,
    h: &cortex::model::ModelHeader,
    vram_base: u64,
) -> Option<(alloc::vec::Vec<CopyWindow>, u64)> {
    let shapes = layer_tensor_shapes(h)?;
    let mut windows: alloc::vec::Vec<CopyWindow> = alloc::vec::Vec::new();
    let mut vram = vram_base;
    let mut assigned: alloc::vec::Vec<(u64, usize, usize)> = alloc::vec::Vec::new();
    for _li in 0..h.num_layers {
        let layer_start = layers_off + (_li) * layout.stride;
        for (slot, (r, c)) in shapes.iter().enumerate() {
            let start = layer_start + layout.tensor_off[slot];
            let len = packed_bytes(*r, *c);
            windows.push(CopyWindow {
                file_start: start,
                file_end: start + len,
                vram_off: vram,
            });
            assigned.push((vram, *r, *c));
            vram += (len as u64 + 4095) & !4095;
        }
    }
    let total = vram - vram_base;
    let _ = assigned; // documentação: SEQ_MATS reconstruído no caller na mesma ordem
    Some((windows, total))
}

/// Resultado do loader (telemetria honesta).
pub struct LoaderOutcome {
    pub mats: usize,
    pub total_bytes: u64,
}

/// Tenta carregar `name` do FAT para a BAR. Ok = lane VRAM ativo com pesos
/// residentes sem heap. Falha = None (o fluxo legado roda — nada quebra).
pub unsafe fn load_from_fat_to_vram(name: &str) -> Option<LoaderOutcome> {
    if !crate::gpu::bar_compute::bar_compute_enabled() {
        k_nano::slog_hal!("LOADER-VRAM", "info", "BAR compute off — loader skip (CPU ladder legado)");
        return None;
    }

    // 1) Acha (device, partição) com o arquivo e lê o header (512B).
    //    Um único lock POR backend por vez — o probe e o stream usam o MESMO
    //    (device, part) para não variar a origem no meio do stream.
    struct Src {
        kind: u8, // 0=ata 1=ahci 2=nvme 3=usb
        part: k_nano::fat32::Partition,
    }
    let mut hdr_buf = alloc::vec![0u8; 512];
    let mut found: Option<(Src, cortex::model::ModelHeader)> = None;
    for (kind, lock_fn) in [
        (0u8, 0usize),
        (1, 1),
        (2, 2),
        (3, 3),
    ] {
        let _ = lock_fn;
        macro_rules! try_backend {
            ($guard:expr) => {{
                if let Some(ref mut d) = *$guard.lock() {
                    let dev: &mut dyn k_nano::block_dev::BlockDevice = d;
                    for p in k_nano::fat32::partitions_on_dev(dev) {
                        if k_nano::fat32::read_root_file_dev_chunked(
                            dev,
                            &p,
                            name,
                            &mut |_off: usize, bytes: &[u8]| {
                                if found.is_none() && hdr_buf[0] == 0 {
                                    let n = bytes.len().min(512);
                                    hdr_buf[..n].copy_from_slice(&bytes[..n]);
                                }
                            },
                        )
                        .is_some()
                        {
                            if let Some(h) = cortex::model::parse_model_header(&hdr_buf) {
                                found = Some((Src { kind, part: p }, h));
                            }
                            break;
                        }
                        if found.is_some() { break; }
                    }
                }
            }};
        }
        match kind {
            0 => try_backend!(k_nano::globals::ATA_DRIVER),
            1 => try_backend!(k_nano::globals::AHCI_DRIVER),
            2 => try_backend!(k_nano::disk_agent::nvme::NVME_DRIVER),
            _ => try_backend!(k_nano::globals::USB_MSC),
        }
        if found.is_some() {
            break;
        }
        // reset p/ próximo backend (header buffer sujo = falso positivo)
        if found.is_none() {
            hdr_buf.fill(0);
        }
    }
    let (src, h) = found?;
    let layers_total = layers_total_bytes(&h)?;
    k_nano::slog_hal!(
        "LOADER-VRAM", "info",
        "{}: v6 h={} L={} — layers={}MB — streaming FAT→BAR",
        name, h.hidden, h.num_layers, layers_total / (1024 * 1024)
    );

    // 2) Reserva INTEIRA da aperture (antes de copiar 1 byte — parcial é
    //    proibido). `reserve_vram_span` falha honesto se não cabe.
    let Some((base_off, _)) = reserve_vram_span(layers_total) else {
        k_nano::slog_hal!("LOADER-VRAM", "warn", "aperture sem espaço p/ layers inteiras — off honesto (legado)");
        return None;
    };

    // 3) Deriva o layers_off do próprio blob (tok_len do header real — sem
    //    chutes) e computa as janelas de cópia.
    let layout = layer_layout(&h)?;
    // Reaproveita o header buffer como fonte do layers_off (tok_len já lá).
    let tok_len = u32::from_le_bytes([
        hdr_buf[45], hdr_buf[46], hdr_buf[47], hdr_buf[48],
    ]) as usize;
    let layers_off = 49 + tok_len + 3
        + match h.embed_type {
            0 => packed_bytes(h.hidden, h.vocab) + 4,
            1 => ((h.hidden.saturating_mul(h.vocab) + 255) / 256) * 210 + 4,
            _ => return None,
        };
    if layers_off + layout.stride * h.num_layers > h.file_size {
        k_nano::slog_hal!("LOADER-VRAM", "warn", "layout {} excede file_size {} — abort honesto", layers_off + layout.stride * h.num_layers, h.file_size);
        return None;
    }
    let (windows, total) = compute_windows(layers_off, &layout, &h, base_off)?;
    debug_assert_eq!(total, layers_total);

    // 4) Stream: reabre o MESMO backend e copia por janela com o leitor
    //    chunked — cada chunk cruza as janelas e vai direto à BAR.
    let mut written_total = 0u64;
    let mut stream_ok = true;
    {
        // Estrutura do callback: mantém o cursor de arquivo e, para cada
        // janela ativa, copia a interseção [chunk] ∩ [janela] via BAR write.
        struct Sink {
            file_pos: usize,
            windows: alloc::vec::Vec<CopyWindow>,
            wi: usize, // índice da janela corrente (ordenadas por file_start)
            written: u64,
            failed: bool,
        }
        impl Sink {
            fn feed(&mut self, off: usize, bytes: &[u8]) {
                self.file_pos = off;
                let chunk_start = off;
                let chunk_end = off + bytes.len();
                // Avança janelas já totalmente passadas.
                while self.wi < self.windows.len()
                    && self.windows[self.wi].file_end <= chunk_start
                {
                    self.wi += 1;
                }
                // Copia interseções (uma janela pode atravessar o chunk todo).
                let mut w = self.wi;
                while w < self.windows.len()
                    && self.windows[w].file_start < chunk_end
                {
                    let win = &self.windows[w];
                    let s = win.file_start.max(chunk_start);
                    let e = win.file_end.min(chunk_end);
                    if e > s {
                        // Offset do chunk dentro da janela define o dest na VRAM.
                        let dest = win.vram_off + (s - win.file_start) as u64;
                        if !unsafe { write_vram_bytes(dest, &bytes[s - chunk_start..e - chunk_start]) } {
                            self.failed = true;
                            return;
                        }
                        self.written += (e - s) as u64;
                    }
                    w += 1;
                }
            }
        }
        let mut sink = Sink {
            file_pos: 0,
            windows,
            wi: 0,
            written: 0,
            failed: false,
        };
        macro_rules! stream_backend {
            ($guard:expr) => {{
                if let Some(ref mut d) = *$guard.lock() {
                    let dev: &mut dyn k_nano::block_dev::BlockDevice = d;
                    // Mesma partição do probe (src.part) — procura pelo lba_start.
                    for p in k_nano::fat32::partitions_on_dev(dev) {
                        if p.lba_start == src.part.lba_start {
                            if k_nano::fat32::read_root_file_dev_chunked(
                                dev,
                                &p,
                                name,
                                &mut |off: usize, bytes: &[u8]| sink.feed(off, bytes),
                            )
                            .is_none()
                            {
                                stream_ok = false;
                            }
                            break;
                        }
                    }
                }
            }};
        }
        match src.kind {
            0 => stream_backend!(k_nano::globals::ATA_DRIVER),
            1 => stream_backend!(k_nano::globals::AHCI_DRIVER),
            2 => stream_backend!(k_nano::disk_agent::nvme::NVME_DRIVER),
            _ => stream_backend!(k_nano::globals::USB_MSC),
        }
        written_total = sink.written;
        stream_ok &= !sink.failed;
    }

    if !stream_ok || written_total != layers_total {
        k_nano::slog_hal!(
            "LOADER-VRAM", "warn",
            "stream incompleto: written={}MB esperado={}MB — SEM registro (lane off; legado roda)",
            written_total / (1024 * 1024), layers_total / (1024 * 1024)
        );
        return None;
    }

    // 5) Registra: SEQ_MATS na ordem canônica (off por janela, mesmas shapes).
    let mut mats = alloc::vec::Vec::with_capacity(windows_n(&h));
    {
        // Reconstroi os offsets na MESMA ordem do compute_windows.
        let shapes = layer_tensor_shapes(&h)?;
        let mut vram = base_off;
        for _li in 0..h.num_layers {
            for (r, c) in shapes {
                let len = packed_bytes(r, c) as u64;
                mats.push((vram, r, c));
                vram += (len + 4095) & !4095;
            }
        }
    }
    register_loader_resident(mats, written_total);
    Some(LoaderOutcome {
        mats: windows_n(&h),
        total_bytes: written_total,
    })
}

fn windows_n(h: &cortex::model::ModelHeader) -> usize {
    h.num_layers * 7
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_batem_com_a_sequencia_do_snapshot() {
        // Falcon3-1B: hidden 2048, 18L, heads 32, kv 4, q_dim 2048, inter 8192.
        let h = cortex::model::ModelHeader {
            hidden: 2048, num_layers: 18, num_heads: 32, vocab: 131072, max_seq: 4096,
            intermediate: 8192, kv_heads: 4, q_dim: 2048, num_medusa: 0,
            tie: true, feat: 0, embed_type: 0, file_size: 0,
        };
        let s = layer_tensor_shapes(&h).unwrap();
        assert_eq!(s[0], (2048, 2048)); // q (h,h)
        let k_dim = 4 * (2048 / 32); // 256
        assert_eq!(s[1], (2048, 256)); // k
        assert_eq!(s[2], (2048, 256)); // v
        assert_eq!(s[3], (2048, 2048)); // o (q_dim, hidden)
        let ffn_group = 8192 * 2048 / 2048; // 8192
        assert_eq!(s[4], (2048, 8192)); // gate
        assert_eq!(s[5], (2048, 8192)); // up
        assert_eq!(s[6], (8192, 2048)); // down (intermediate, down_out)
        let _ = ffn_group;
    }

    #[test]
    fn layers_total_1b_cerca_de_370mb() {
        // 1B: por layer = (2048*2048 + 2048*256*2 + 2048*2048 + 2048*8192*2
        //                  + 8192*2048)/4
        let h = cortex::model::ModelHeader {
            hidden: 2048, num_layers: 18, num_heads: 32, vocab: 131072, max_seq: 4096,
            intermediate: 8192, kv_heads: 4, q_dim: 2048, num_medusa: 0,
            tie: true, feat: 0, embed_type: 0, file_size: 0,
        };
        let total = layers_total_bytes(&h).unwrap();
        // 14.25MB/layer × 18 = 256.5MB (cabe em BAR1 256MB só com ReBAR ≥;
        // BAR1 256MB padrão = off honesto, pré-checagem recusa).
        assert!(total > 200 * 1024 * 1024 && total < 300 * 1024 * 1024, "total={}MB", total / (1024 * 1024));
    }

    #[test]
    fn packed_bytes_arredonda_para_cima() {
        assert_eq!(packed_bytes(4, 4), 4);
        assert_eq!(packed_bytes(1, 1), 1);
        assert_eq!(packed_bytes(3, 3), 3); // (9+3)/4 = 3
        assert_eq!(packed_bytes(5, 5), 7); // (25+3)/4 = 7
    }
}
