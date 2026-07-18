//! NeuOS Probe fase 1 (ADR-0047 Pilar 3) — snapshot-safe weight summary.
//! Nunca muta pesos do modelo ativo. Heurística Healthy/Degraded por layer.

use alloc::string::String;
use alloc::vec::Vec;
use crate::cortex::TransformerModel;
use crate::tensor::PackedTernaryTensor;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LayerStatus {
    Healthy,
    Degraded,
    Absent,
}

pub struct LayerReport {
    pub index: usize,
    pub status: LayerStatus,
    pub mean_abs: f32,
    pub std_approx: f32,
}

/// Soul-vector stub: 7 scalars.
pub struct SoulVector {
    pub dims: [f32; 7],
}

pub struct ProbeReport {
    pub layers: Vec<LayerReport>,
    pub soul: SoulVector,
    pub summary: String,
}

fn packed_stats(t: &PackedTernaryTensor) -> (f32, f32) {
    let n = t.packed_data.len().min(256);
    if n == 0 {
        return (0.0, 0.0);
    }
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    for i in 0..n {
        // Sample decoded weights (4 per byte)
        for k in 0..4 {
            let idx = i * 4 + k;
            let w = t.get_weight(idx) as f32;
            let a = if w >= 0.0 { w } else { -w };
            sum += a;
            sum_sq += a * a;
        }
    }
    let count = (n * 4) as f32;
    let mean = sum / count;
    let var = (sum_sq / count) - mean * mean;
    let std = if var > 0.0 { libm::sqrtf(var) } else { 0.0 };
    (mean, std)
}

/// Probe read-only weight stats for first `max_layers` layers.
pub fn probe_model(model: &TransformerModel, max_layers: usize) -> ProbeReport {
    let n = model.layers.len().min(max_layers);
    let mut layers = Vec::with_capacity(n);
    let mut healthy = 0u32;
    let mut mean_acc = 0.0f32;
    let mut std_acc = 0.0f32;

    for i in 0..n {
        let layer = &model.layers[i];
        let (m_gate, s_gate) = packed_stats(&layer.gate);
        let (m_up, s_up) = packed_stats(&layer.up);
        let mean_abs = (m_gate + m_up) * 0.5;
        let std_approx = (s_gate + s_up) * 0.5;

        let status = if layer.gate.packed_data.is_empty() {
            LayerStatus::Absent
        } else if mean_abs > 1.5 || std_approx > 1.2 {
            LayerStatus::Degraded
        } else {
            LayerStatus::Healthy
        };
        if status == LayerStatus::Healthy {
            healthy += 1;
        }
        mean_acc += mean_abs;
        std_acc += std_approx;
        layers.push(LayerReport {
            index: i,
            status,
            mean_abs,
            std_approx,
        });
    }

    let inv = if n > 0 { 1.0 / n as f32 } else { 0.0 };
    let health_ratio = if n > 0 { healthy as f32 * inv } else { 0.0 };
    let soul = SoulVector {
        dims: [
            mean_acc * inv,
            std_acc * inv,
            health_ratio,
            model.layers.len() as f32,
            model.hidden as f32 / 1024.0,
            model.num_heads as f32 / 32.0,
            1.0,
        ],
    };

    let summary = alloc::format!(
        "layers={}/{} healthy={} soul=[{:.3},{:.3},{:.3}]",
        n,
        model.layers.len(),
        healthy,
        soul.dims[0],
        soul.dims[1],
        soul.dims[2]
    );

    ProbeReport {
        layers,
        soul,
        summary,
    }
}

pub fn log_probe(model: Option<&TransformerModel>) {
    match model {
        None => {
            k_nano::slog_cortex!("ADR", "0047-L3", "probe=NO_MODEL");
        }
        Some(m) => {
            if m.layers.is_empty() {
                k_nano::slog_cortex!("ADR", "0047-L3", "probe=NO_MODEL");
                return;
            }
            let report = probe_model(m, 8.min(m.layers.len()));
            k_nano::slog_cortex!("ADR", "0047-L3", "probe=OK {}", report.summary);
            for lr in report.layers.iter().take(4) {
                let st = match lr.status {
                    LayerStatus::Healthy => "H",
                    LayerStatus::Degraded => "D",
                    LayerStatus::Absent => "A",
                };
                k_nano::slog_cortex!("PROBE", "info", "L{} {} mean={:.4} std={:.4}", lr.index, st, lr.mean_abs, lr.std_approx);
            }
        }
    }
}
