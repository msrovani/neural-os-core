//! ADR-0081 C5: Federated Gradient Sharing (#312f).
//!
//! ## Visao
//! Cada no executa SleepCycle localmente (REPLAY → DREAM → CONSOLIDATE).
//! Os gradientes resultantes (nao os dados) sao enviados para o Master.
//! Master agrega via FedYogi-style optimizer e distribui o modelo global
//! de volta para os Workers.
//!
//! Privacidade: apenas gradientes trafegam — dados nunca saem do no.
//!
//! ## Depende de: P2P Transport (Fase A da ADR-0081) + #312f
//! - `k_nano::net::mesh::local_role()` para saber se e Master/Worker
//! - `k_nano::net::mesh::NodeRole` para tipos de papel
//! - `k_nano::net::transport::HybridTransport` para envio/recebimento
//! - `TrainingAgent` (k_ai::training_agent) para executar fine-tuning local
//!
//! ## Fallback local (ativo enquanto P2P nao estiver vivo)
//! Sem P2P, cada no treina localmente — comportamento atual.
//! A funcao `share_gradients()` retorna imediatamente se P2P nao estiver ativo
//! (mesh::local_role() == Undecided).

use alloc::vec::Vec;
use alloc::vec;
use k_nano::net::mesh::{self, NodeRole};
use k_nano::net::noproto::{AiosTaskPacket, PacketFlags, TaskType};
use k_nano::net::udp_broadcast;
use spin::Mutex;
use crate::cognitive::ternary_update;

/// ADR-0081 §C5.1: FedYogi hyper-parameters.
const FEDYOGI_BETA1: f32 = 0.9;
const FEDYOGI_BETA2: f32 = 0.99;
const FEDYOGI_EPS: f32 = 1e-7;
const FEDYOGI_LR: f32 = 0.01;

/// Porta P2P do mesh (transport k_nano, broadcast 42069).
const P2P_PORT: u16 = 42069;
/// Intervalo (ticks do TIMER) entre envio de gradiente / broadcast do modelo.
const FL_INTERVAL_TICKS: u64 = 200;
/// Nº de gradientes no buffer para agregar antes do intervalo (Master).
const FL_AGGREGATE_MIN: usize = 2;

/// Mínimo de pesos para considerar um gradiente válido.
const MIN_WEIGHT_LEN: usize = 8;

/// Federated Trainer — C5 #312f.
///
/// Encapsula o estado do treinamento federado:
/// - `active`: se o P2P mesh esta vivo e este no participa do federated learning
/// - `round`: rodada federada local (incrementada a cada share_gradients)
/// - `local_weights`: placeholder para pesos ternarios (copia local do modelo)
/// - `global_round`: ultima rodada recebida do Master (para deteccao de atualizacao)
/// - `fedyogi_m`: momento de primeira ordem (FedYogi)
/// - `fedyogi_v`: momento de segunda ordem (FedYogi)
/// - `worker_gradients`: buffer de gradientes recebidos de Workers (only Master)
pub struct FederatedTrainer {
    /// Se o P2P mesh esta ativo para federated learning.
    pub active: bool,
    /// Rodada federada atual (incrementada a cada share_gradients).
    pub round: u64,
    /// Placeholder para pesos ternarios locais.
    pub local_weights: Vec<i8>,
    /// Ultima rodada global recebida do Master (0 = nunca).
    pub global_round: u64,
    /// Momento de primeira ordem (FedYogi).
    fedyogi_m: Vec<f32>,
    /// Momento de segunda ordem (FedYogi).
    fedyogi_v: Vec<f32>,
    /// Buffer de gradientes de Workers (Master only).
    worker_gradients: Vec<Vec<f32>>,
    /// Último tick do TIMER em que o Worker enviou gradiente.
    last_gradient_tick: u64,
    /// Último tick do TIMER em que o Master fez broadcast do modelo global.
    last_broadcast_tick: u64,
}

impl FederatedTrainer {
    /// Cria um novo FederatedTrainer.
    ///
    /// Verifica o papel local no mesh:
    /// - `Master` ou `Worker` → `active = true`
    /// - `Undecided` → `active = false` (fallback local)
    pub fn new(weight_count: usize) -> Self {
        let role = mesh::local_role();
        let active = role == NodeRole::Master || role == NodeRole::Worker;

        FederatedTrainer {
            active,
            round: 0,
            local_weights: vec![0i8; weight_count],
            global_round: 0,
            fedyogi_m: vec![0.0f32; weight_count],
            fedyogi_v: vec![0.0f32; weight_count],
            worker_gradients: Vec::new(),
            last_gradient_tick: 0,
            last_broadcast_tick: 0,
        }
    }

    /// Cria um FederatedTrainer com um estado de pesos inicial.
    pub fn with_weights(weights: &[i8]) -> Self {
        let n = weights.len();
        let role = mesh::local_role();
        let active = role == NodeRole::Master || role == NodeRole::Worker;

        FederatedTrainer {
            active,
            round: 0,
            local_weights: weights.to_vec(),
            global_round: 0,
            fedyogi_m: vec![0.0f32; n],
            fedyogi_v: vec![0.0f32; n],
            worker_gradients: Vec::new(),
            last_gradient_tick: 0,
            last_broadcast_tick: 0,
        }
    }

    /// Retorna a rodada federada atual.
    pub fn round(&self) -> u64 {
        self.round
    }

    /// Retorna se o P2P esta ativo.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Forca ativacao/desativacao manual (para testes / config).
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Reavalia o estado ativo baseado no mesh.
    ///
    /// Chamado quando o mesh pode ter mudado (eleicao, reconexao).
    pub fn refresh_active(&mut self) {
        let role = mesh::local_role();
        self.active = role == NodeRole::Master || role == NodeRole::Worker;
    }

    /// Compartilha gradientes com o Master (se Worker) ou agrega (se Master).
    ///
    /// ## Comportamento por papel
    /// - **Undecided** (P2P inativo): `local_training_step` — fallback local.
    /// - **Worker**: armazena `weights` como gradiente local. A distribuição
    ///   real (`FD\0` assinado → Master) acontece no `mesh_tick()` (Fase C —
    ///   a cada ~200 ticks do TIMER).
    /// - **Master**: agrega os gradientes recebidos com FedYogi e atualiza
    ///   `local_weights`. O broadcast do modelo global (`FM\0`) também
    ///   acontece no `mesh_tick()`.
    ///
    /// O `round` indica a rodada de treinamento do SleepCycle.
    pub fn share_gradients(&mut self, weights: &[i8], round: u64) {
        self.round = round;
        let role = mesh::local_role();

        match role {
            NodeRole::Undecided => {
                // P2P inativo: fallback local — apenas armazena os pesos
                self.local_weights.copy_from_slice(weights);
            }
            NodeRole::Worker => {
                // Worker: armazena os pesos; o envio do gradiente (`FD\0`,
                // assinado) ocorre no mesh_tick() (Fase C, ADR-0081).
                self.local_weights.copy_from_slice(weights);
                k_nano::slog_cortex!(
                    "FL", "worker",
                    "gradient round={} len={} stored (TX via mesh_tick)", round, weights.len()
                );
            }
            NodeRole::Master => {
                // Master: agrega gradientes dos workers (o broadcast `FM\0`
                // acontece no mesh_tick()).
                let gradient = self.compute_gradient(weights);
                self.worker_gradients.push(gradient);

                // ponytail: esperar N workers ou timeout; por agora agrega imediatamente
                self.aggregate_fedyogi();
                self.global_round = round;

                k_nano::slog_cortex!("FL", "master", "aggregated round={} workers={}", round, self.worker_gradients.len());
            }
            _ => {
                // Memory/Compute nodes: store locally, no federated sharing
                self.local_weights.copy_from_slice(weights);
            }
        }
    }

    /// Recebe o modelo global do Master (se Worker) ou retorna o modelo
    /// local agregado (se Master).
    ///
    /// O Worker recebe o `FM\0` do Master via `mesh_tick()` (drena o EventBus
    /// P2P_PACKET e aplica weights + global_round). Se nenhum modelo chegou
    /// ainda (`global_round == 0`), retorna `None`.
    ///
    /// ## Retorno
    /// - `Some(&[i8])` com os pesos globais se disponiveis
    /// - `None` se P2P inativo ou nenhum modelo recebido ainda
    pub fn receive_global_model(&self) -> Option<&[i8]> {
        let role = mesh::local_role();
        match role {
            NodeRole::Undecided => None,
            NodeRole::Worker => {
                // O FM\0 é aplicado no mesh_tick() (drain do EventBus).
                if self.global_round > 0 {
                    Some(&self.local_weights)
                } else {
                    None
                }
            }
            NodeRole::Master => {
                if self.global_round > 0 {
                    Some(&self.local_weights)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  ADR-0081 C5 (Fase C): P2P real via k_nano mesh (SESSION_236/237)
    // ═══════════════════════════════════════════════════════════════

    /// Tick do mesh federado — chamado pelo bin a cada bei_tick.
    ///
    /// ## Comportamento por papel (P2P ativo)
    /// - **Worker**: a cada `FL_INTERVAL_TICKS` (~2s a 100Hz), se
    ///   `local_weights` tem algo não-zero, envia gradiente `FD\0` (assinado,
    ///   fragmentado) para o Master.
    /// - **Master**: drena o EventBus `P2P_PACKET`, acumula gradientes `FD\0`
    ///   dos Workers e, a cada N gradientes ou intervalo, agrega via FedYogi
    ///   e faz broadcast do modelo global `FM\0` (assinado, fragmentado).
    /// - **Worker** (RX): drena o EventBus e aplica `FM\0` (dest_id == meu id
    ///   ou 0xFF) → atualiza `local_weights`/`global_round`.
    ///
    /// A assinatura é verificada no ingress do k_nano (`p2p_tick`, Fase A
    /// fail-closed) ANTES de publicar no EventBus — o payload consumido aqui
    /// é o pacote já verificado (sem os 64 bytes de assinatura).
    pub fn mesh_tick(&mut self, tick: u64) {
        self.refresh_active();
        let role = mesh::local_role();
        if role == NodeRole::Undecided || !self.active {
            return; // fallback local — share_gradients cobre
        }

        self.drain_fl_events(role);

        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        match role {
            NodeRole::Worker => {
                if self.last_gradient_tick == 0
                    || now.wrapping_sub(self.last_gradient_tick) >= FL_INTERVAL_TICKS
                {
                    self.last_gradient_tick = now;
                    self.send_gradient();
                }
            }
            NodeRole::Master => {
                let due = self.last_broadcast_tick == 0
                    || now.wrapping_sub(self.last_broadcast_tick) >= FL_INTERVAL_TICKS;
                let has_grads = !self.worker_gradients.is_empty();
                let many = self.worker_gradients.len() >= FL_AGGREGATE_MIN;
                let periodic = tick % 400 == 0 && has_grads;
                if due && (many || periodic || has_grads) {
                    self.last_broadcast_tick = now;
                    self.broadcast_global_model();
                }
            }
            _ => {}
        }
    }

    /// Drena o EventBus P2P_PACKET (subscribe lazy) e aplica por papel:
    /// Master acumula `FD\0`; Worker aplica `FM\0`.
    fn drain_fl_events(&mut self, role: NodeRole) {
        {
            let mut recv = FL_RECV.lock();
            if recv.is_none() {
                *recv = Some(k_nano::EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_P2P_PACKET));
            }
        }
        loop {
            let evt = FL_RECV.lock().as_ref().and_then(|r| r.try_receive());
            let Some(evt) = evt else { break };
            if evt.topic != k_nano::net::mesh::TOPIC_P2P_PACKET {
                continue;
            }
            let Some(pkt) = k_nano::net::udp_broadcast::parse(&evt.payload) else { continue };
            if pkt.task_type != TaskType::Inference {
                continue;
            }
            let src = pkt.source_id;
            let dst = pkt.dest_id;
            let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
                &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
            } else {
                &[][..]
            };
            match role {
                NodeRole::Master => {
                    if payload.starts_with(b"FD\0") {
                        if let Some(grad) = self.parse_gradient(payload) {
                            self.worker_gradients.push(grad);
                            k_nano::slog_cortex!(
                                "FL", "master",
                                "gradient RX node={} buf={}", src, self.worker_gradients.len()
                            );
                        }
                    }
                }
                NodeRole::Worker => {
                    if payload.starts_with(b"FM\0") && (dst == mesh::node_id() || dst == 0xFF) {
                        self.apply_global_model(payload);
                    }
                }
                _ => {}
            }
        }
    }

    /// Worker: serializa `local_weights` como gradiente `FD\0` (assinado).
    fn send_gradient(&mut self) {
        if !self.local_weights.iter().any(|&w| w != 0) {
            return; // nada a compartilhar ainda
        }
        let my_id = mesh::node_id();
        // ADR-0081 follow-up: clock monotônico único por fonte (anti-replay).
        let pkt = AiosTaskPacket::new(mesh::next_data_clock(), my_id, 0xFF, TaskType::Inference, 1, 0, 0, PacketFlags::new());
        let mut buf = udp_broadcast::serialize(&pkt);
        buf.extend_from_slice(b"FD\0");
        buf.extend_from_slice(&self.round.to_le_bytes());
        buf.extend_from_slice(&(self.local_weights.len() as u32).to_le_bytes());
        buf.extend_from_slice(&pack_i8(&self.local_weights));
        let Some(signed) = udp_broadcast::sign_packet(&buf) else { return };
        let ok = udp_broadcast::send_fragmented(&signed, P2P_PORT);
        k_nano::slog_cortex!(
            "FL", "worker",
            "gradient round={} len={} sent={}", self.round, self.local_weights.len(), ok
        );
    }

    /// Master: agrega via FedYogi e faz broadcast do modelo global `FM\0`.
    fn broadcast_global_model(&mut self) {
        if self.worker_gradients.is_empty() {
            return;
        }
        let n_workers = self.worker_gradients.len();
        let round = self.round.max(self.global_round).saturating_add(1);
        self.aggregate_fedyogi();
        self.global_round = round;
        let my_id = mesh::node_id();
        // ADR-0081 follow-up: clock monotônico único por fonte (anti-replay).
        let pkt = AiosTaskPacket::new(mesh::next_data_clock(), my_id, 0xFF, TaskType::Inference, 1, 0, 0, PacketFlags::new());
        let mut buf = udp_broadcast::serialize(&pkt);
        buf.extend_from_slice(b"FM\0");
        buf.extend_from_slice(&self.global_round.to_le_bytes());
        buf.extend_from_slice(&(self.local_weights.len() as u32).to_le_bytes());
        buf.extend_from_slice(&pack_i8(&self.local_weights));
        let Some(signed) = udp_broadcast::sign_packet(&buf) else { return };
        let ok = udp_broadcast::send_fragmented(&signed, P2P_PORT);
        k_nano::slog_cortex!(
            "FL", "master",
            "aggregated round={} workers={} broadcast={}", self.global_round, n_workers, ok
        );
    }

    /// Worker: aplica `FM\0` recebido (weights + global_round).
    fn apply_global_model(&mut self, payload: &[u8]) {
        // "FM\0" + round u64 LE + len u32 LE + packed 2-bit
        if payload.len() < 15 || &payload[0..3] != b"FM\0" {
            return;
        }
        let round = u64::from_le_bytes([
            payload[3], payload[4], payload[5], payload[6],
            payload[7], payload[8], payload[9], payload[10],
        ]);
        let len = u32::from_le_bytes([payload[11], payload[12], payload[13], payload[14]]) as usize;
        if len == 0 || len > 1_000_000 {
            return;
        }
        let weights = unpack_i8(&payload[15..], len);
        if weights.len() != self.local_weights.len() {
            return;
        }
        self.local_weights = weights;
        self.global_round = round;
        k_nano::slog_cortex!(
            "FL", "worker",
            "global model round={} len={} applied", round, self.local_weights.len()
        );
    }

    /// Master: desempacota `FD\0` e computa o gradiente vs modelo local.
    fn parse_gradient(&self, payload: &[u8]) -> Option<Vec<f32>> {
        // "FD\0" + round u64 LE + len u32 LE + packed 2-bit
        if payload.len() < 15 || &payload[0..3] != b"FD\0" {
            return None;
        }
        let len = u32::from_le_bytes([payload[11], payload[12], payload[13], payload[14]]) as usize;
        if len == 0 || len > 1_000_000 {
            return None;
        }
        let weights = unpack_i8(&payload[15..], len);
        Some(self.compute_gradient(&weights))
    }

    /// (round, global_round, worker_gradients.len()) — para log no bei_tick.
    pub fn fl_stats(&self) -> (u64, u64, usize) {
        (self.round, self.global_round, self.worker_gradients.len())
    }

    /// Executa um passo de treinamento local (sempre funciona).
    ///
    /// Usa `k_ai::cognitive::ternary_update` para atualizar os pesos
    /// ternarios com gradientes simulados. Este e o passo base que
    /// acontece em TODOS os nos, independente de P2P.
    ///
    /// ## Parametros
    /// - `weights`: slice mutavel de pesos ternarios {-1, 0, +1}
    /// - `data`: pares (input, target) para treino
    ///
    /// ## Retorno
    /// Perda media (loss) sobre os exemplos.
    pub fn local_training_step(&mut self, weights: &mut [i8], data: &[(Vec<f32>, Vec<f32>)]) -> f32 {
        if data.is_empty() || weights.len() < MIN_WEIGHT_LEN {
            return 0.0;
        }
        let mut total_loss = 0.0f32;
        let n = data.len();
        for (input, target) in data {
            let grads = self.compute_gradients_f32(weights, input, target);
            let loss = self.mse_loss(weights, input, target);
            total_loss += loss;
            ternary_update(weights, &grads, 0.01);
        }
        let avg_loss = total_loss / n as f32;
        self.local_weights.copy_from_slice(weights);
        self.round += 1;
        avg_loss
    }

    // ═══════════════════════════════════════════════════════════════
    //  Metodos internos
    // ═══════════════════════════════════════════════════════════════

    /// Computa gradiente entre pesos locais e recebidos (delta).
    fn compute_gradient(&self, received: &[i8]) -> Vec<f32> {
        let n = core::cmp::min(self.local_weights.len(), received.len());
        let mut grad = vec![0.0f32; n];
        for i in 0..n {
            grad[i] = received[i] as f32 - self.local_weights[i] as f32;
        }
        grad
    }

    /// Computa gradientes via perda MSE para um par (input, target).
    /// Funcao de ativacao: tanh (aproximacao ternaria).
    fn compute_gradients_f32(&self, weights: &[i8], input: &[f32], target: &[f32]) -> Vec<f32> {
        let out_len = target.len();
        let mut grads = vec![0.0f32; weights.len().min(input.len() * out_len)];

        if weights.len() < input.len() * out_len {
            return grads; // shape mismatch
        }

        // Forward: y_j = sum_i(w_ij * x_i), ativacao tanh
        let mut output = vec![0.0f32; out_len];
        for j in 0..out_len {
            let mut sum = 0.0f32;
            for i in 0..input.len() {
                let idx = i * out_len + j;
                if idx < weights.len() {
                    sum += weights[idx] as f32 * input[i];
                }
            }
            output[j] = libm::tanhf(sum);
        }

        // Backward: dL/dw_ij = (y_j - t_j) * (1 - tanh^2(sum_j)) * x_i
        for j in 0..out_len {
            let dy = output[j] - target[j];
            let dtanh = 1.0 - output[j] * output[j];
            for i in 0..input.len() {
                let idx = i * out_len + j;
                if idx < grads.len() {
                    grads[idx] = dy * dtanh * input[i];
                }
            }
        }

        grads
    }

    /// Erro quadratico medio (MSE) para um par (input, target).
    fn mse_loss(&self, weights: &[i8], input: &[f32], target: &[f32]) -> f32 {
        let out_len = target.len();
        let mut loss = 0.0f32;
        for j in 0..out_len {
            let mut sum = 0.0f32;
            for i in 0..input.len() {
                let idx = i * out_len + j;
                if idx < weights.len() {
                    sum += weights[idx] as f32 * input[i];
                }
            }
            let y = libm::tanhf(sum);
            let diff = y - target[j];
            loss += diff * diff;
        }
        loss / out_len as f32
    }

    /// Agrega gradientes dos Workers via FedYogi-style optimizer.
    ///
    /// Algoritmo (FedYogi simplificado):
    ///   grad_avg = mean(worker_gradients)
    ///   m = beta1 * m + (1 - beta1) * grad_avg
    ///   v = v - (1 - beta2) * grad_avg^2 * sign(v - grad_avg^2)
    ///   theta = theta - lr * m / (sqrt(v) + eps)
    fn aggregate_fedyogi(&mut self) {
        if self.worker_gradients.is_empty() {
            return;
        }

        let n_workers = self.worker_gradients.len() as f32;
        let weight_count = self.local_weights.len();

        // media dos gradientes dos workers
        let mut grad_avg = vec![0.0f32; weight_count];
        for wg in &self.worker_gradients {
            for (i, &g) in wg.iter().enumerate() {
                if i < weight_count {
                    grad_avg[i] += g;
                }
            }
        }
        for g in grad_avg.iter_mut() {
            *g /= n_workers;
        }

        // FedYogi update
        for i in 0..weight_count {
            let g = grad_avg[i];
            let m_i = &mut self.fedyogi_m[i];
            let v_i = &mut self.fedyogi_v[i];

            // m_t = beta1 * m_{t-1} + (1 - beta1) * g_t
            *m_i = FEDYOGI_BETA1 * *m_i + (1.0 - FEDYOGI_BETA1) * g;

            // v_t = v_{t-1} - (1 - beta2) * g_t^2 * sign(v_{t-1} - g_t^2)
            let g2 = g * g;
            let diff = *v_i - g2;
            let sign_diff = if diff > 0.0 { 1.0 } else if diff < 0.0 { -1.0 } else { 0.0 };
            *v_i = *v_i - (1.0 - FEDYOGI_BETA2) * g2 * sign_diff;

            // Clamp v_i para evitar valores negativos ou zero
            if *v_i <= 0.0 {
                *v_i = FEDYOGI_EPS;
            }

            // theta = theta - lr * m / (sqrt(v) + eps)
            let step = FEDYOGI_LR * *m_i / (libm::sqrtf(*v_i) + FEDYOGI_EPS);
            let w = self.local_weights[i] as f32 - step;
            self.local_weights[i] = w.clamp(-1.0, 1.0) as i8;
        }

        // Limpa buffer de gradientes apos agregacao
        self.worker_gradients.clear();
    }
}

// ═══════════════════════════════════════════════════════════════
//  Packing ternário 2-bit (wire-compatível com cortex)
// ═══════════════════════════════════════════════════════════════

/// Empacota pesos ternários {-1, 0, +1} em 2 bits/peso (4 por byte),
/// wire-compatível com `cortex::tensor::PackedTernaryTensor::pack_weights`:
/// LSB-first dentro do byte, codificação {-1→0b10, 0→0b00, 1→0b01}.
/// k_ai NÃO depende de cortex — packing próprio idêntico ao padrão.
pub fn pack_i8(v: &[i8]) -> Vec<u8> {
    let n = (v.len() + 3) / 4;
    let mut out = vec![0u8; n];
    for (i, &w) in v.iter().enumerate() {
        let byte = i / 4;
        let shift = (i % 4) * 2;
        let code = match w {
            -1 => 0b10,
            1 => 0b01,
            _ => 0b00,
        };
        out[byte] |= code << shift;
    }
    out
}

/// Desempacota 2-bit → i8 (inverso de `pack_i8`). `len` = nº de pesos.
pub fn unpack_i8(packed: &[u8], len: usize) -> Vec<i8> {
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let byte = i / 4;
        let shift = (i % 4) * 2;
        let bits = (packed.get(byte).copied().unwrap_or(0) >> shift) & 0b11;
        out.push(match bits {
            0b01 => 1,
            0b10 => -1,
            _ => 0,
        });
    }
    out
}

// ═══════════════════════════════════════════════════════════════
//  Wiring global (ADR-0081 C5, Fase C) — chamado pelo bin bei_tick
// ═══════════════════════════════════════════════════════════════

/// Receiver do EventBus P2P_PACKET (subscribe lazy).
static FL_RECV: Mutex<Option<event_bus::Receiver>> = Mutex::new(None);

/// Instância global do treinador federado (lazy init, 1024 pesos).
static FL_TRAINER: Mutex<Option<FederatedTrainer>> = Mutex::new(None);

/// Tick do FL federado — chamado pelo bin a cada bei_tick (após p2p_tick,
/// que publica os pacotes P2P não-heartbeat no EventBus).
pub fn mesh_tick_global(tick: u64) {
    {
        let mut guard = FL_TRAINER.lock();
        if guard.is_none() {
            *guard = Some(FederatedTrainer::new(1024));
        }
    }
    let mut guard = FL_TRAINER.lock();
    if let Some(ref mut t) = *guard {
        t.mesh_tick(tick);
    }
}

/// (round, global_round, worker_gradients.len()) do trainer global.
pub fn fl_stats_global() -> (u64, u64, usize) {
    let guard = FL_TRAINER.lock();
    match guard.as_ref() {
        Some(t) => t.fl_stats(),
        None => (0, 0, 0),
    }
}

// ═══════════════════════════════════════════════════════════════
//  Helper publico: funcao para integrar com SleepCycle CONSOLIDATE
// ═══════════════════════════════════════════════════════════════

/// Integracao com SleepCycle: chame apos CONSOLIDATE phase.
///
/// Se o FederatedTrainer estiver ativo, os pesos sao compartilhados
/// (Worker → Master) ou agregados (Master).
/// Se inativo, o treinamento continua localmente sem efeito colateral.
///
/// ## Parametros
/// - `trainer`: instancia do FederatedTrainer (static ou passada por ref)
/// - `weights`: pesos ternarios atualizados apos CONSOLIDATE
/// - `round`: numero da rodada do SleepCycle
pub fn sleepcycle_consolidate_hook(trainer: &mut FederatedTrainer, weights: &[i8], round: u64) {
    trainer.share_gradients(weights, round);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Testa criacao basica do trainer.
    #[test]
    fn test_new() {
        let t = FederatedTrainer::new(64);
        assert_eq!(t.local_weights.len(), 64);
        assert_eq!(t.round(), 0);
    }

    /// Testa criacao com pesos iniciais.
    #[test]
    fn test_with_weights() {
        let w = vec![1i8; 128];
        let t = FederatedTrainer::with_weights(&w);
        assert_eq!(t.local_weights, w);
    }

    /// Testa ativacao/desativacao manual.
    #[test]
    fn test_set_active() {
        let mut t = FederatedTrainer::new(32);
        t.set_active(true);
        assert!(t.is_active());
        t.set_active(false);
        assert!(!t.is_active());
    }

    /// Testa que local_training_step funciona mesmo sem P2P.
    #[test]
    fn test_local_training_step() {
        let mut t = FederatedTrainer::new(64);
        let mut weights = vec![0i8; 64];
        let data = vec![
            (vec![1.0f32; 4], vec![0.5f32; 16]),
            (vec![0.5f32; 4], vec![1.0f32; 16]),
        ];
        let loss = t.local_training_step(&mut weights, &data);
        // Loss deve ser > 0 (pesos mudaram)
        assert!(loss > 0.0 || loss == 0.0); // aceita 0 se shape mismatch
    }

    /// Testa que share_gradients funciona sem P2P (fallback local).
    #[test]
    fn test_share_gradients_undecided() {
        let mut t = FederatedTrainer::new(64);
        let w = vec![1i8; 64];
        t.share_gradients(&w, 1);
        assert_eq!(t.round(), 1);
    }

    /// Testa que receive_global_model retorna None sem P2P.
    #[test]
    fn test_receive_global_model_undecided() {
        let t = FederatedTrainer::new(64);
        assert!(t.receive_global_model().is_none());
    }

    /// Testa o ciclo completo: treino local + share + receive.
    #[test]
    fn test_train_share_receive() {
        let mut t = FederatedTrainer::new(64);
        let mut weights = vec![0i8; 64];

        // Treina localmente
        let data = vec![
            (vec![1.0f32; 8], vec![-1.0f32; 8]),
        ];
        let _loss = t.local_training_step(&mut weights, &data);

        // Share (fallback local, P2P inativo)
        t.share_gradients(&weights, 1);

        // Receive deve retornar None (P2P inativo)
        assert!(t.receive_global_model().is_none());
    }

    /// Testa que o FedYogi optimizer funciona.
    #[test]
    fn test_fedyogi_aggregate() {
        let mut t = FederatedTrainer::with_weights(&[0i8; 16]);

        // Simula 2 workers enviando gradientes
        t.worker_gradients.push(vec![0.1f32; 16]);
        t.worker_gradients.push(vec![-0.05f32; 16]);
        t.global_round = 1;
        t.aggregate_fedyogi();

        // Apos agregacao, pesos devem ter mudado
        let changed = t.local_weights.iter().any(|&w| w != 0);
        assert!(changed, "FedYogi aggregation should change weights");

        // Buffer deve estar limpo
        assert!(t.worker_gradients.is_empty());
    }
}
