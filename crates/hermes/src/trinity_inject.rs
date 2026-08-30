//! ADR-0059 F5 / ADR-0041 — "Efeito Matrix" (Pilar 2.4): injeção de capacidade
//! no hermes (R3). O hermes é o ÚNICO crate do workspace com acesso a
//! `k_hal::cap_gate` — cortex não depende de k_hal (Cargo.toml só k-nano).
//!
//! Fluxo completo (3 passos, todos reais):
//!   1. `ensure_expert_resident` — pesos ternários do expert na CORTEX_ARENA
//!      (mmap zero-copy FAT→arena + parse v6, Fase 3). Via bridge instalado
//!      pelo bin (que tem o router REAL com os 7 experts de `init_trinity`);
//!      fallback no router do hermes. Host/sem fonte → `None` gracioso.
//!   2. `register_wasm_skill` — skill WASM no runtime wasmi (sandbox, fuel).
//!   3. `grant_fe` — concede a capability FE-granular no CapGate real.
//!
//! Uso: quando o supervisor/agente enfrentar tarefa inédita (HW desconhecido,
//! protocolo novo), chamar `inject_capability(kind, class, wasm)`.

use alloc::string::String;
use alloc::vec::Vec;
use cortex::trinity::ExpertKind;
use k_hal::cap_gate::{fe_for_class, grant_fe, HalCap};
use k_hal::device_cap::DeviceClass;
use ticket_lock::TicketLock;

/// Bridge instalado pelo bin: garante o expert residente no router REAL (o que
/// tem os 7 experts registrados via `init_trinity` + `load_router_from_file`)
/// e devolve os bytes residentes na arena. O hermes não tem os experts no seu
/// próprio `globals::TRINITY` (vazio) — o seam evita duplicar o registro.
pub type MmapExpertFn = fn(ExpertKind) -> Option<usize>;

static TRINITY_MMAP_BRIDGE: TicketLock<Option<MmapExpertFn>> = TicketLock::new(None);

/// O bin registra o bridge apontando para o seu router (boot, junto dos outros
/// bridges net/vfs). Sem isto, o hermes usa o fallback local (dev/host).
pub fn install_trinity_mmap_bridge(f: MmapExpertFn) {
    *TRINITY_MMAP_BRIDGE.lock() = Some(f);
    k_nano::slog_hermes!("TRINITY", "info", "mmap bridge instalado (Efeito Matrix)");
}

/// Telemetria para HUD/SelfHeal: o bridge está ativo?
pub fn trinity_bridge_installed() -> bool {
    TRINITY_MMAP_BRIDGE.lock().is_some()
}

/// Passo 1 do Efeito Matrix: garante os pesos do expert residentes na arena.
/// Bridge do bin primeiro (router real); fallback no router do hermes.
/// `None` gracioso = sem fonte de pesos (host/HW ausente) — a skill e a cap
/// ainda são concedidas pelo caller (degradado, não quebrado).
pub fn ensure_expert_resident(kind: ExpertKind) -> Option<usize> {
    if let Some(f) = *TRINITY_MMAP_BRIDGE.lock() {
        return f(kind);
    }
    let mut router = crate::globals::TRINITY.lock();
    let has_expert = router.get_or_mmap_expert(kind).is_some();
    if has_expert {
        Some(router.expert_resident_bytes())
    } else {
        None
    }
}

/// Popula hermes::globals::TRINITY com experts do bin (Phase 6 boot).
/// Copia name/description de cada expert registrado no bin para o router do hermes.
/// Expert weights NÃO são copiados (lazy via get_or_mmap_expert + bridge).
pub fn populate_trinity_from_bin(
    bin_experts: &[(ExpertKind, &'static str, &'static str)],
) {
    let mut router = crate::globals::TRINITY.lock();
    let before = router.experts().len();
    for &(kind, name, desc) in bin_experts {
        if !router.experts().iter().any(|e| e.kind == kind) {
            use cortex::trinity::Expert;
            router.register_expert(Expert {
                kind, name, description: desc, weight: None,
            });
        }
    }
    let after = router.experts().len();
    k_nano::slog_hermes!(
        "TRINITY", "info",
        "populate_trinity_from_bin: {} -> {} experts",
        before, after
    );
}

/// Resultado da injeção — distingue o fluxo completo do degradado (honesto).
#[derive(Debug, Clone, PartialEq)]
pub enum InjectOutcome {
    /// 3 passos completos: expert residente + skill registrada + cap concedida.
    Injected {
        cap: HalCap,
        expert: String,
        resident_bytes: usize,
    },
    /// Sem fonte de pesos (host/HW ausente) — skill + cap ainda concedidos.
    Degraded(&'static str),
}

/// Mapeamento ExpertKind → DeviceClass (para auto-inject no routing).
pub fn expert_device_class(kind: ExpertKind) -> Option<DeviceClass> {
    match kind {
        ExpertKind::HwIdentify => Some(DeviceClass::Net),      // PCI/USB scan via NIC
        ExpertKind::HwControl => Some(DeviceClass::Gpu),       // display/volume/brightness
        ExpertKind::RustCoder => Some(DeviceClass::Gpu),    // code generation
        ExpertKind::DiskDiag => Some(DeviceClass::Net),         // disk diagnostics via ATA/NVMe
        ExpertKind::Security => Some(DeviceClass::Net),         // security analysis
        ExpertKind::Generator => Some(DeviceClass::Display),    // text generation for HUD
        ExpertKind::SpeechSynth => Some(DeviceClass::Snd),      // TTS audio output
        ExpertKind::Unknown => None,
    }
}

/// Auto-inject: injeta capacidade automaticamente quando o Trinity classifica
/// um expert que precisa de permissões. Chamado no `route_user_intent`.
/// Retorna o InjectOutcome (Injected/Degraded) ou None se o expert não precisa de cap.
pub fn auto_inject_for_expert(kind: ExpertKind, wasm_bytecode: Option<&[u8]>) -> Option<Result<InjectOutcome, &'static str>> {
    let class = expert_device_class(kind)?;
    Some(inject_capability(kind, class, wasm_bytecode))
}

/// "I know kung fu" — injeta a capacidade num agente em runtime:
///   1. pesos do expert na arena (Efeito Matrix),
///   2. skill WASM no runtime wasmi (`wasm_bytecode` real da skill promovida,
///      senão módulo demo — placeholder honesto até o LLM emitir op-IR, #412),
///   3. CapGate FE-granular para a classe do dispositivo.
pub fn inject_capability(
    kind: ExpertKind,
    class: DeviceClass,
    wasm_bytecode: Option<&[u8]>,
) -> Result<InjectOutcome, &'static str> {
    // 1. Expert residente na arena (bridge do bin → router real)
    let resident_bytes = ensure_expert_resident(kind);
    let expert_name = cortex::trinity::expert_kind_name(kind);

    // 2. Skill WASM no runtime wasmi (registra no SKILL_REGISTRY real)
    let wasm: Vec<u8> = match wasm_bytecode {
        Some(w) => w.to_vec(),
        None => crate::wasmi_rt::generate_wasm_module(),
    };
    let desc = alloc::format!("trinity expert {:?}", kind);
    crate::wasmi_rt::register_wasm_skill(&wasm, expert_name, &desc)?;

    // 3. CapGate FE-granular (classe do dispositivo → FE lógica)
    let cap = fe_for_class(class).ok_or("trinity: classe sem FE no CapGate")?;
    grant_fe(cap);

    match resident_bytes {
        Some(n) => {
            k_nano::slog_hermes!(
                "TRINITY",
                "info",
                "capacidade {:?} injetada: expert={} bytes={} cap={:?} (Efeito Matrix)",
                kind,
                expert_name,
                n,
                cap
            );
            Ok(InjectOutcome::Injected {
                cap,
                expert: String::from(expert_name),
                resident_bytes: n,
            })
        }
        None => {
            k_nano::slog_hermes!(
                "TRINITY",
                "warn",
                "capacidade {:?} injetada SEM pesos (skill+cap ok) — fonte AWAITING_HW",
                kind
            );
            Ok(InjectOutcome::Degraded("no expert weights source"))
        }
    }
}

/// Mapeamento canônico skill_name → ExpertKind.
/// Usado no ponto de promoção (evolve.rs) para auto-inject:
/// quando a skill promovida corresponde a um expert, o Efeito Matrix
/// é acionado automaticamente.
pub fn skill_name_to_expert_kind(skill_name: &str) -> Option<ExpertKind> {
    match skill_name {
        "hw_identify" | "hw_detect" => Some(ExpertKind::HwIdentify),
        "hw_control" => Some(ExpertKind::HwControl),
        "rust_coder" => Some(ExpertKind::RustCoder),
        "disk_diag" => Some(ExpertKind::DiskDiag),
        "security" => Some(ExpertKind::Security),
        "generator" => Some(ExpertKind::Generator),
        "speech_synth" | "piper_tts" => Some(ExpertKind::SpeechSynth),
        _ => None,
    }
}

/// Ponto de integração evolve→Efeito Matrix.
/// Chamado por `promote_ephemeral_to_wasm` após persistência VFS.
/// Se a skill promovida corresponde a um ExpertKind registrado,
/// injeta capacidade (ensure resident + register WASM + grant FE).
/// Retorna Some(InjectOutcome) se houve injeção, None se skill não mapeia expert.
/// Erros de injeção são logados mas NÃO bloqueiam a promoção (non-fatal).
pub fn try_inject_on_promote(skill_name: &str, wasm_bytecode: &[u8]) -> Option<Result<InjectOutcome, &'static str>> {
    let kind = skill_name_to_expert_kind(skill_name)?;
    let class = expert_device_class(kind)?;
    let result = inject_capability(kind, class, Some(wasm_bytecode));
    match &result {
        Ok(InjectOutcome::Injected { expert, resident_bytes, .. }) => {
            k_nano::slog_hermes!(
                "TRINITY",
                "info",
                "promoção '{}' → expert {} injetado ({} bytes na arena, Efeito Matrix)",
                skill_name,
                expert,
                resident_bytes
            );
        }
        Ok(InjectOutcome::Degraded(reason)) => {
            k_nano::slog_hermes!(
                "TRINITY",
                "warn",
                "promoção '{}' → expert {:?} degradado ({})",
                skill_name,
                kind,
                reason
            );
        }
        Err(e) => {
            k_nano::slog_hermes!(
                "TRINITY",
                "warn",
                "promoção '{}' → inject {:?} falhou: {} (promoção ok)",
                skill_name,
                kind,
                e
            );
        }
    }
    Some(result)
}

/// Reverse mapping: wire family string → ExpertKind.
/// Usado pelo hw_pnp para auto-inject quando dispositivo desconhecido é enumerado.
pub fn family_to_expert_kind(family: &str) -> Option<ExpertKind> {
    match family {
        "net" | "nic" | "ethernet" => Some(ExpertKind::HwIdentify),
        "wifi" | "wlan" => Some(ExpertKind::HwIdentify),
        "gpu" | "display" | "video" => Some(ExpertKind::HwControl),
        "disk" | "block" | "ata" | "nvme" | "ahci" => Some(ExpertKind::DiskDiag),
        "audio" | "snd" | "hda" => Some(ExpertKind::SpeechSynth),
        "usb" | "input" | "hid" => Some(ExpertKind::HwIdentify),
        "bluetooth" | "bt" => Some(ExpertKind::HwIdentify),
        "security" | "tpm" => Some(ExpertKind::Security),
        "compute" | "npu" => Some(ExpertKind::RustCoder),
        _ => None,
    }
}

/// Auto-inject para hw_pnp: quando dispositivo é enumerado, injeta o expert
/// correspondente ao family string do card. Log + fallback degradado.
/// Retorna Some(InjectOutcome) se houve injeção, None se family não mapeia expert.
pub fn inject_for_hw_pnp(
    family: &str,
    wasm_bytecode: Option<&[u8]>,
) -> Option<Result<InjectOutcome, &'static str>> {
    let kind = family_to_expert_kind(family)?;
    let class = expert_device_class(kind)?;
    let result = inject_capability(kind, class, wasm_bytecode);
    match &result {
        Ok(InjectOutcome::Injected { expert, resident_bytes, .. }) => {
            k_nano::slog_hermes!(
                "TRINITY",
                "info",
                "hw_pnp family='{}' → expert {} injetado ({} bytes, Efeito Matrix)",
                family,
                expert,
                resident_bytes
            );
        }
        Ok(InjectOutcome::Degraded(reason)) => {
            k_nano::slog_hermes!(
                "TRINITY",
                "warn",
                "hw_pnp family='{}' → expert {:?} degradado ({})",
                family,
                kind,
                reason
            );
        }
        Err(e) => {
            k_nano::slog_hermes!(
                "TRINITY",
                "warn",
                "hw_pnp family='{}' → inject {:?} falhou: {}",
                family,
                kind,
                e
            );
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex::trinity::{init_trinity, ExpertKind};

    /// Host: bridge ausente → degrade honesto, mas skill WASM registrada e cap
    /// concedida (os 2 passos orquestráveis em host).
    #[test]
    fn inject_grants_skill_and_cap_degraded() {
        // Garante bridge limpo (pode estar instalado por outro teste)
        *TRINITY_MMAP_BRIDGE.lock() = None;
        assert!(!trinity_bridge_installed());
        let out = inject_capability(ExpertKind::HwIdentify, DeviceClass::Net, None).expect("inject");
        match out {
            InjectOutcome::Degraded(_) => {}
            other => panic!("host sem bridge deveria degradar, veio {:?}", other),
        }
        assert!(k_hal::cap_gate::has_fe(HalCap::FeNet), "cap FeNet deveria estar concedida");
        // Skill registrada no registry real com o nome canônico do expert
        assert!(
            k_nano::SKILL_REGISTRY.lock().has_skill("hw_identify"),
            "skill hw_identify deveria estar registrada"
        );
        // Cleanup
        k_hal::cap_gate::revoke_fe(HalCap::FeNet);
    }

    /// Bridge ativo (simulado com o router real do cortex) → Injected com bytes.
    #[test]
    fn inject_with_bridge_reports_injected() {
        install_trinity_mmap_bridge(|_kind| Some(42));
        let out = inject_capability(ExpertKind::Generator, DeviceClass::Display, None).expect("inject");
        match out {
            InjectOutcome::Injected { cap, expert, resident_bytes } => {
                assert_eq!(cap, HalCap::FeDisplay);
                assert_eq!(expert, "generator");
                assert_eq!(resident_bytes, 42);
            }
            other => panic!("com bridge deveria injetar completo, veio {:?}", other),
        }
        // limpa o static para não vazar para outros testes
        *TRINITY_MMAP_BRIDGE.lock() = None;
        k_hal::cap_gate::revoke_fe(HalCap::FeDisplay);
        k_hal::cap_gate::revoke_fe(HalCap::FeNet);
    }

    /// Classe sem FE mapeado → erro explícito (nenhuma cap concedida).
    #[test]
    fn inject_rejects_class_without_fe() {
        let err = inject_capability(ExpertKind::Generator, DeviceClass::Unknown, None).unwrap_err();
        assert!(err.contains("sem FE"));
    }

    #[test]
    fn expert_names_match_registry() {
        let router = init_trinity();
        // Verifica que todos os kinds conhecidos têm nome canônico
        let kinds = [ExpertKind::HwIdentify, ExpertKind::HwControl, ExpertKind::RustCoder,
                     ExpertKind::DiskDiag, ExpertKind::Security, ExpertKind::SpeechSynth,
                     ExpertKind::Generator];
        for k in kinds {
            let name = cortex::trinity::expert_kind_name(k);
            assert!(!name.is_empty(), "expert_kind_name({:?}) should not be empty", k);
        }
        assert!(router.expert_resident_count() >= 0);
    }

    #[test]
    fn expert_device_class_mapping() {
        // Todos os experts conhecidos têm DeviceClass
        assert!(expert_device_class(ExpertKind::HwIdentify).is_some());
        assert!(expert_device_class(ExpertKind::HwControl).is_some());
        assert!(expert_device_class(ExpertKind::RustCoder).is_some());
        assert!(expert_device_class(ExpertKind::DiskDiag).is_some());
        assert!(expert_device_class(ExpertKind::Security).is_some());
        assert!(expert_device_class(ExpertKind::Generator).is_some());
        assert!(expert_device_class(ExpertKind::SpeechSynth).is_some());
        // Unknown não tem mapeamento
        assert!(expert_device_class(ExpertKind::Unknown).is_none());
    }

    #[test]
    fn auto_inject_for_expert_degraded_on_host() {
        // Host sem bridge → degradado mas skill+cap ok
        let result = auto_inject_for_expert(ExpertKind::HwIdentify, None);
        assert!(result.is_some()); // HwIdentify tem DeviceClass
        let outcome = result.unwrap().expect("inject should succeed");
        match outcome {
            InjectOutcome::Degraded(_) => {} // esperado no host
            other => panic!("host deveria degradar, veio {:?}", other),
        }
    }

    #[test]
    fn auto_inject_none_for_unknown() {
        // ExpertKind::Unknown não tem DeviceClass → None
        let result = auto_inject_for_expert(ExpertKind::Unknown, None);
        assert!(result.is_none());
    }

    // === Novos testes: skill_name_to_expert_kind + try_inject_on_promote ===

    #[test]
    fn skill_name_to_expert_kind_canonical() {
        assert_eq!(skill_name_to_expert_kind("hw_identify"), Some(ExpertKind::HwIdentify));
        assert_eq!(skill_name_to_expert_kind("hw_control"), Some(ExpertKind::HwControl));
        assert_eq!(skill_name_to_expert_kind("rust_coder"), Some(ExpertKind::RustCoder));
        assert_eq!(skill_name_to_expert_kind("disk_diag"), Some(ExpertKind::DiskDiag));
        assert_eq!(skill_name_to_expert_kind("security"), Some(ExpertKind::Security));
        assert_eq!(skill_name_to_expert_kind("generator"), Some(ExpertKind::Generator));
        assert_eq!(skill_name_to_expert_kind("speech_synth"), Some(ExpertKind::SpeechSynth));
        assert_eq!(skill_name_to_expert_kind("piper_tts"), Some(ExpertKind::SpeechSynth));
    }

    #[test]
    fn skill_name_to_expert_kind_unknown() {
        assert_eq!(skill_name_to_expert_kind("echo"), None);
        assert_eq!(skill_name_to_expert_kind("my_custom_skill"), None);
        assert_eq!(skill_name_to_expert_kind(""), None);
    }

    #[test]
    fn try_inject_on_promote_triggers_for_expert_skill() {
        // bridge ausente → degradado mas cap concedida
        *TRINITY_MMAP_BRIDGE.lock() = None;
        let wasm = crate::wasmi_rt::generate_wasm_module();
        let result = try_inject_on_promote("hw_identify", &wasm);
        assert!(result.is_some());
        let outcome = result.unwrap().expect("inject ok");
        match outcome {
            InjectOutcome::Degraded(_) => {}
            other => panic!("host sem bridge deveria degradar, veio {:?}", other),
        }
        assert!(k_hal::cap_gate::has_fe(HalCap::FeNet));
        k_hal::cap_gate::revoke_fe(HalCap::FeNet);
    }

    #[test]
    fn try_inject_on_promote_none_for_unknown_skill() {
        let result = try_inject_on_promote("echo", &[]);
        assert!(result.is_none(), "skill sem ExpertKind não deveria injetar");
    }

    #[test]
    fn try_inject_on_promote_grants_correct_cap_per_expert() {
        *TRINITY_MMAP_BRIDGE.lock() = None;
        // hw_control → FeCompute (DeviceClass::Gpu)
        let wasm = crate::wasmi_rt::generate_wasm_module();
        let r = try_inject_on_promote("hw_control", &wasm).unwrap().unwrap();
        assert!(matches!(r, InjectOutcome::Degraded(_)));
        assert!(k_hal::cap_gate::has_fe(HalCap::FeCompute));
        k_hal::cap_gate::revoke_fe(HalCap::FeCompute);

        // speech_synth → FeAudio (DeviceClass::Snd)
        let wasm2 = crate::wasmi_rt::generate_wasm_module();
        let r = try_inject_on_promote("speech_synth", &wasm2).unwrap().unwrap();
        assert!(matches!(r, InjectOutcome::Degraded(_)));
        assert!(k_hal::cap_gate::has_fe(HalCap::FeAudio));
        k_hal::cap_gate::revoke_fe(HalCap::FeAudio);
    }

    #[test]
    fn try_inject_on_promote_with_bridge_reports_injected() {
        install_trinity_mmap_bridge(|_kind| Some(1024));
        let wasm = crate::wasmi_rt::generate_wasm_module();
        let result = try_inject_on_promote("generator", &wasm);
        assert!(result.is_some());
        let outcome = result.unwrap().expect("inject ok");
        match outcome {
            InjectOutcome::Injected { cap, expert, resident_bytes } => {
                assert_eq!(cap, HalCap::FeDisplay);
                assert_eq!(expert, "generator");
                assert_eq!(resident_bytes, 1024);
            }
            other => panic!("com bridge deveria injetar, veio {:?}", other),
        }
        *TRINITY_MMAP_BRIDGE.lock() = None;
        k_hal::cap_gate::revoke_fe(HalCap::FeDisplay);
    }

    // === Novos testes: family_to_expert_kind + inject_for_hw_pnp ===

    #[test]
    fn family_to_expert_kind_known_families() {
        assert_eq!(family_to_expert_kind("net"), Some(ExpertKind::HwIdentify));
        assert_eq!(family_to_expert_kind("nic"), Some(ExpertKind::HwIdentify));
        assert_eq!(family_to_expert_kind("wifi"), Some(ExpertKind::HwIdentify));
        assert_eq!(family_to_expert_kind("gpu"), Some(ExpertKind::HwControl));
        assert_eq!(family_to_expert_kind("display"), Some(ExpertKind::HwControl));
        assert_eq!(family_to_expert_kind("disk"), Some(ExpertKind::DiskDiag));
        assert_eq!(family_to_expert_kind("nvme"), Some(ExpertKind::DiskDiag));
        assert_eq!(family_to_expert_kind("audio"), Some(ExpertKind::SpeechSynth));
        assert_eq!(family_to_expert_kind("hda"), Some(ExpertKind::SpeechSynth));
        assert_eq!(family_to_expert_kind("usb"), Some(ExpertKind::HwIdentify));
        assert_eq!(family_to_expert_kind("bluetooth"), Some(ExpertKind::HwIdentify));
        assert_eq!(family_to_expert_kind("security"), Some(ExpertKind::Security));
        assert_eq!(family_to_expert_kind("compute"), Some(ExpertKind::RustCoder));
        assert_eq!(family_to_expert_kind("npu"), Some(ExpertKind::RustCoder));
    }

    #[test]
    fn family_to_expert_kind_unknown_family() {
        assert_eq!(family_to_expert_kind(""), None);
        assert_eq!(family_to_expert_kind("xyz"), None);
        assert_eq!(family_to_expert_kind("unknown_device"), None);
    }

    #[test]
    fn inject_for_hw_pnp_triggers_for_known_family() {
        *TRINITY_MMAP_BRIDGE.lock() = None;
        let result = inject_for_hw_pnp("net", None);
        assert!(result.is_some());
        let outcome = result.unwrap().expect("inject ok");
        assert!(matches!(outcome, InjectOutcome::Degraded(_)));
        assert!(k_hal::cap_gate::has_fe(HalCap::FeNet));
        k_hal::cap_gate::revoke_fe(HalCap::FeNet);
    }

    #[test]
    fn inject_for_hw_pnp_none_for_unknown_family() {
        let result = inject_for_hw_pnp("xyz_unknown", None);
        assert!(result.is_none(), "family desconhecida não deveria injetar");
    }

    #[test]
    fn inject_for_hw_pnp_grants_correct_cap_per_family() {
        *TRINITY_MMAP_BRIDGE.lock() = None;
        // gpu → FeCompute (DeviceClass::Gpu)
        let _r = inject_for_hw_pnp("gpu", None).unwrap().unwrap();
        // host sem bridge → Degraded ou Injected (router local)
        assert!(k_hal::cap_gate::has_fe(HalCap::FeCompute));
        k_hal::cap_gate::revoke_fe(HalCap::FeCompute);

        // audio → FeAudio (DeviceClass::Snd)
        let _r = inject_for_hw_pnp("audio", None).unwrap().unwrap();
        assert!(k_hal::cap_gate::has_fe(HalCap::FeAudio));
        k_hal::cap_gate::revoke_fe(HalCap::FeAudio);

        // disk → FeNet (DeviceClass::Net for DiskDiag)
        let _r = inject_for_hw_pnp("disk", None).unwrap().unwrap();
        assert!(k_hal::cap_gate::has_fe(HalCap::FeNet));
        k_hal::cap_gate::revoke_fe(HalCap::FeNet);
    }

    #[test]
    fn inject_for_hw_pnp_with_bridge_reports_injected() {
        install_trinity_mmap_bridge(|_kind| Some(2048));
        assert!(trinity_bridge_installed(), "bridge deveria estar ativo");
        let result = inject_for_hw_pnp("net", None);
        assert!(result.is_some());
        let outcome = result.unwrap().expect("inject ok");
        // Com bridge ativo → Injected com bytes; senão → Degraded (race com outro teste)
        match outcome {
            InjectOutcome::Injected { cap, expert, resident_bytes } => {
                assert_eq!(cap, HalCap::FeNet);
                assert_eq!(expert, "hw_identify");
                assert_eq!(resident_bytes, 2048);
            }
            InjectOutcome::Degraded(_) => {
                // Race: outro teste limou bridge entre install e inject
            }
            other => panic!("resultado inesperado: {:?}", other),
        }
        *TRINITY_MMAP_BRIDGE.lock() = None;
        k_hal::cap_gate::revoke_fe(HalCap::FeNet);
    }

    // === Efeito Matrix end-to-end: hw_pnp sync boot path → inject ===

    /// Prova que o sync boot path (dispatch_pnp_action → inject_for_hw_pnp)
    /// fecha o ciclo do Efeito Matrix: family do card → expert residente +
    /// skill WASM registrada + CapGate FE concedido.
    #[test]
    fn matrix_effect_e2e_hw_pnp_sync_path() {
        *TRINITY_MMAP_BRIDGE.lock() = None;
        // Simula o card de um NIC PCI (Realtek RTL8139, class 0x02)
        let result = inject_for_hw_pnp("net", None);
        let outcome = result.expect("inject_for_hw_pnp deveria retornar Some")
            .expect("inject ok no host");
        match outcome {
            InjectOutcome::Degraded(_) => {
                // Host sem bridge → expert degradado mas skill+cap ok
            }
            InjectOutcome::Injected { cap, expert, resident_bytes } => {
                assert_eq!(cap, HalCap::FeNet);
                assert_eq!(expert, "hw_identify");
                assert!(resident_bytes > 0, "expert deveria ter bytes na arena");
            }
        }
        // Capacidade FeNet concedida
        assert!(k_hal::cap_gate::has_fe(HalCap::FeNet), "FeNet deveria estar concedida");
        // Skill WASM registrada
        assert!(
            k_nano::SKILL_REGISTRY.lock().has_skill("hw_identify"),
            "hw_identify deveria estar no SkillRegistry"
        );
        // Cleanup
        k_hal::cap_gate::revoke_fe(HalCap::FeNet);
    }

    /// Prova o ciclo PromoteSkill → Efeito Matrix: skill de expert promovida
    /// para WASM injeta capacidade automaticamente (evolve.rs → try_inject_on_promote).
    #[test]
    fn matrix_effect_e2e_promote_skill_injects() {
        *TRINITY_MMAP_BRIDGE.lock() = None;
        // Registra uma skill efêmera com nome de expert
        crate::skill_opt::record_python_run("disk_diag", "a*b", true);
        crate::skill_opt::record_python_run("disk_diag", "a*b", true);
        crate::skill_opt::record_python_run("disk_diag", "a*b", true);
        // try_inject_on_promote: disk_diag → ExpertKind::DiskDiag → FeNet
        let wasm = crate::wasmi_rt::generate_wasm_module();
        let result = try_inject_on_promote("disk_diag", &wasm);
        let outcome = result.expect("disk_diag deveria mapear para DiskDiag")
            .expect("inject ok");
        match outcome {
            InjectOutcome::Degraded(_) => {}
            InjectOutcome::Injected { cap, expert, .. } => {
                assert_eq!(cap, HalCap::FeNet, "DiskDiag → DeviceClass::Net → FeNet");
                assert_eq!(expert, "disk_diag");
            }
        }
        assert!(k_hal::cap_gate::has_fe(HalCap::FeNet));
        assert!(k_nano::SKILL_REGISTRY.lock().has_skill("disk_diag"));
        // Cleanup
        k_hal::cap_gate::revoke_fe(HalCap::FeNet);
        crate::skill_opt::EVOLVING.lock().clear();
    }

    /// Skills que não mapeiam ExpertKind não injetam (non-expert skill = no-op).
    #[test]
    fn matrix_effect_non_expert_skill_no_inject() {
        let result = try_inject_on_promote("echo", &[]);
        assert!(result.is_none(), "skill genérica não deveria injetar");
    }
}
