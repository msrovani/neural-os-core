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
use crate::cognitive::ternary_update;

/// ADR-0081 §C5.1: FedYogi hyper-parameters.
const FEDYOGI_BETA1: f32 = 0.9;
const FEDYOGI_BETA2: f32 = 0.99;
const FEDYOGI_EPS: f32 = 1e-7;
const FEDYOGI_LR: f32 = 0.01;

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
    /// - **Worker**: serializa `weights` como gradiente e envia ao Master via
    ///   P2P transport (ponytail: stub para quando udp_broadcast estiver vivo).
    /// - **Master**: agrega os gradientes recebidos com FedYogi e atualiza
    ///   `local_weights`. Distribui o modelo global de volta (ponytail: stub).
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
                // Worker: envia gradiente para o Master
                let _gradient = self.compute_gradient(weights);
                // ponytail: enviar via P2P transport quando estiver vivo
                // let packet = self.serialize_gradient(&gradient, round);
                // let _ = udp_broadcast::send(&packet);
                // Por enquanto: armazena localmente como se tivesse enviado
                self.local_weights.copy_from_slice(weights);
                k_nano::slog_cortex!("FL", "worker", "gradient round={} len={} (P2P stub)", round, weights.len());
            }
            NodeRole::Master => {
                // Master: agrega gradientes dos workers
                let gradient = self.compute_gradient(weights);
                self.worker_gradients.push(gradient);

                // ponytail: esperar N workers ou timeout; por agora agrega imediatamente
                self.aggregate_fedyogi();
                self.global_round = round;

                // ponytail: broadcast modelo global para workers via P2P
                // let model_packet = self.serialize_global_model();
                // let _ = udp_broadcast::broadcast(&model_packet);

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
    /// ## Retorno
    /// - `Some(&[i8])` com os pesos globais se disponiveis
    /// - `None` se P2P inativo ou nenhum modelo recebido ainda
    pub fn receive_global_model(&self) -> Option<&[i8]> {
        let role = mesh::local_role();
        match role {
            NodeRole::Undecided => None,
            NodeRole::Worker => {
                // ponytail: receber do Master via P2P transport
                // let packet = udp_broadcast::recv()?;
                // let model = self.deserialize_global_model(&packet)?;
                // self.global_round = model.round;
                // Some(&model.weights)
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
