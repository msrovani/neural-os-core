//! Roteamento MoE Sistema 1 / Sistema 2 (manifesto NixAIOS Cortex/Hermes).
//! S1 = reação local (Active / experts leves). S2 = raciocínio profundo (GeneratorPro).
//! Heurística keyword até router neural treinado — honesty: não afirma TTFT <15ms.

use nos_core::ModelSlot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemLane {
    /// Latência-alvo local; nunca bloqueia heartbeat com trabalho pesado.
    S1Local,
    /// Offload síncrono-marcado (async real = residual até fila dedicada).
    S2Deep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteDecision {
    pub slot: ModelSlot,
    pub lane: SystemLane,
    pub reason: &'static str,
}

/// Classifica intenção → slot + lane. Fail-closed: default S1 Active.
pub fn route_intent(prompt: &str) -> RouteDecision {
    let p = prompt.to_ascii_lowercase();
    let complex = p.len() > 280
        || contains_any(
            &p,
            &[
                "refator",
                "refactor",
                "arquitetur",
                "architecture",
                "implemente",
                "implement ",
                "desenvolva",
                "analise profunda",
                "análise profunda",
                "prove que",
                "theorem",
                "migrate",
                "migrar",
            ],
        );

    if contains_any(&p, &["pci", "hwid", "driver", "firmware", "gpu ", "nvidia", "amdgpu"]) {
        return RouteDecision {
            slot: ModelSlot::HwExpert,
            lane: SystemLane::S1Local,
            reason: "hw_keywords",
        };
    }
    if contains_any(&p, &["rust ", "cargo ", "crate ", "wasm", "borrow"]) {
        return RouteDecision {
            slot: ModelSlot::RustCoder,
            lane: if complex {
                SystemLane::S2Deep
            } else {
                SystemLane::S1Local
            },
            reason: "rust_keywords",
        };
    }
    if complex {
        return RouteDecision {
            slot: ModelSlot::GeneratorPro,
            lane: SystemLane::S2Deep,
            reason: "complex_or_long",
        };
    }
    RouteDecision {
        slot: ModelSlot::Active,
        lane: SystemLane::S1Local,
        reason: "default_s1",
    }
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_is_s1_active() {
        let d = route_intent("oi");
        assert_eq!(d.slot, ModelSlot::Active);
        assert_eq!(d.lane, SystemLane::S1Local);
    }

    #[test]
    fn long_arch_is_s2_pro() {
        let d = route_intent(
            "refatore a arquitetura do cortex para migrar o hub MoE e prove que o roteamento \
             Sistema 2 não bloqueia o heartbeat do scheduler no NixAIOS appliance",
        );
        assert_eq!(d.slot, ModelSlot::GeneratorPro);
        assert_eq!(d.lane, SystemLane::S2Deep);
    }

    #[test]
    fn pci_to_hw() {
        let d = route_intent("qual driver nvidia para este pci");
        assert_eq!(d.slot, ModelSlot::HwExpert);
    }
}
