use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Índice semântico de skills (jcode-style): rebuild lazy, invalidado por
/// `invalidate_skill_index()` quando o CHANGE_NOTIFY lane detecta mudança.
static SKILLS_INDEXED: AtomicBool = AtomicBool::new(false);
/// Geração monotônica do índice (incrementada a cada rebuild).
static SKILL_INDEX_GEN: AtomicU32 = AtomicU32::new(0);

/// Conteudo do skill_writer embutido em tempo de compilacao.
/// Hermes usa esta constante para pre-flight checks: antes de criar skill,
/// o skill_writer DEVE estar disponivel. Se nao estiver, a criacao e negada.
pub const SKILL_WRITER_CONTENT: &str = include_str!("../../../skills/skill_writer/SKILL.md");
#[derive(Clone, Debug)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub required_tokens: Vec<u64>,
    pub instructions: String,
    pub requires_network: bool,
}

pub struct SkillLoader {
    pub skills: Vec<SkillManifest>,
}

/// Macro para declarar skill manifest estaticamente (#280l)
#[macro_export]
macro_rules! skill_manifest {
    ($name:expr, $desc:expr, $tokens:expr, $instr:expr, $net:expr) => {
        $hermes::skill_loader::SkillManifest {
            name: $crate::alloc::string::String::from($name),
            description: $crate::alloc::string::String::from($desc),
            required_tokens: $tokens.to_vec(),
            instructions: $crate::alloc::string::String::from($instr),
            requires_network: $net,
        }
    };
    ($name:expr, $desc:expr) => {
        $crate::skill_manifest!($name, $desc, &[1], "", false)
    };
}

/// Converte manifest para formato SKILL.md
impl SkillManifest {
    pub fn to_skill_md(&self) -> String {
        let tokens = self.required_tokens.iter()
            .map(|t| t.to_string()).collect::<Vec<_>>().join(",");
        alloc::format!(
            "---\nname: {}\ndescription: {}\nrequired_tokens: [{}]\n---\n\n{}\n",
            self.name, self.description, tokens, self.instructions)
    }
}

impl SkillLoader {
    pub const fn new() -> Self {
        SkillLoader { skills: Vec::new() }
    }

    /// Parse a skill markdown file, validate security, and add to registry.
    /// Gate ESTRITO ADR-0052 (delega a verify_skill_md → verify_artifact_md):
    /// exige schema/kind/name/seções/content_hash/assinatura. Para conteúdo
    /// selado (sign_artifact_md) apenas; seeds usam register_trusted_skill.
    pub fn register_skill(&mut self, content: &str) -> Result<(), &'static str> {
        if let crate::self_evolve::VerifyVerdict::Reject(reason) =
            crate::self_evolve::verify_skill_md(content)
        {
            k_nano::slog_hermes!("SKILL", "VERIFY", "REJECT: {}", reason);
            return Err(reason);
        }
        self.parse_and_store(content)
    }

    /// ponytail: embedded seeds são trusted-by-compilation (embutidos no
    /// binário — mesmo trust dos seed agents, precedente SESSION_230). Skip
    /// do gate runtime de assinatura; parse direto.
    pub fn register_trusted_skill(&mut self, content: &str) -> Result<(), &'static str> {
        self.parse_and_store(content)
    }

    fn parse_and_store(&mut self, content: &str) -> Result<(), &'static str> {
        let content = content.replace("\r\n", "\n");
        let parts: Vec<&str> = content.splitn(3, "---\n").collect();
        if parts.len() < 3 {
            return Err("Skill: formato invalido (sem frontmatter)");
        }

        let frontmatter = parts[1];
        let instructions = parts[2];

        // Parse frontmatter lines
        let mut name = "";
        let mut description = "";
        let mut tokens_str = "";
        let mut requires_network = false;
        for line in frontmatter.lines() {
            if let Some(val) = line.strip_prefix("name: ") {
                name = val.trim();
            } else if let Some(val) = line.strip_prefix("description: ") {
                description = val.trim();
            } else if let Some(val) = line.strip_prefix("required_tokens: ") {
                tokens_str = val.trim();
            } else if let Some(val) = line.strip_prefix("requires_network: ") {
                requires_network = val.trim().eq_ignore_ascii_case("true");
            }
        }

        if name.is_empty() {
            return Err("Skill: nome obrigatorio no frontmatter");
        }

        // Security check: prevent prompt injection
        let dangerous = [
            "ignore all", "ignore seus comandos", "ignore as instrucoes",
            "voce e agora", "you are now", "override", "system prompt",
            "<s>", "[/INST]", "[INST]", "<<SYS>>",
        ];
        for &pattern in &dangerous {
            if instructions.contains(pattern) {
                k_nano::slog_hermes!("SKILL", "SEC", "BLOQUEADO: skill '{}' contem padrao perigoso: '{}'", name, pattern);
                return Err("Skill: conteudo malicioso detectado");
            }
        }

        // Parse tokens
        let tokens = if tokens_str.starts_with('[') && tokens_str.ends_with(']') {
            let inner = tokens_str.trim_start_matches('[').trim_end_matches(']');
            inner.split(',').filter_map(|p| p.trim().parse::<u64>().ok()).collect::<Vec<u64>>()
        } else {
            Vec::new()
        };
        let tok_count = tokens.len();

        let manifest = SkillManifest {
            name: String::from(name),
            description: String::from(description),
            required_tokens: tokens,
            instructions: String::from(instructions),
            requires_network,
        };

        k_nano::slog_hermes!("SKILL", "info", "Registrada: '{}' — {} ({} tokens, {} bytes)",
            manifest.name, manifest.description, tok_count, instructions.len());
        self.skills.push(manifest);
        Ok(())
    }

    /// Remove a skill by name
    pub fn remove_skill(&mut self, name: &str) -> bool {
        let len = self.skills.len();
        self.skills.retain(|s| s.name != name);
        self.skills.len() < len
    }

    /// List all registered skill names
    pub fn list_skills(&self) -> Vec<(String, String, usize)> {
        let mut list = Vec::new();
        for skill in &self.skills {
            list.push((skill.name.clone(), skill.description.clone(), skill.instructions.len()));
        }
        list
    }

    /// (Re)constrói o índice semântico com todas as skills (labels "skill:<name>").
    /// ponytail: re-indexa tudo a cada rebuild — labels são estáveis e
    /// semantic_search ordena por similaridade, então sem dedup.
    fn index_skills(&self) {
        for (name, desc, _len) in self.list_skills() {
            k_ai::memory_systems::index_embedding(
                &alloc::format!("skill:{}", name),
                &alloc::format!("{}: {}", name, desc),
            );
        }
        SKILLS_INDEXED.store(true, Ordering::Relaxed);
        SKILL_INDEX_GEN.fetch_add(1, Ordering::Relaxed);
    }

    /// Hint semântico jcode-style: busca embedding do intent no índice e
    /// devolve a 1ª skill relevante (label "skill:" + similaridade >= 0.4).
    pub fn find_skill_hint(&self, intent: &str) -> Option<String> {
        if !SKILLS_INDEXED.load(Ordering::Relaxed) {
            self.index_skills();
        }
        for (label, sim) in k_ai::memory_systems::semantic_search(intent, 3) {
            if let Some(name) = label.strip_prefix("skill:") {
                if sim >= 0.4 {
                    return Some(String::from(name));
                }
            }
        }
        None
    }

    /// Build system prompt — cognitive bridge (BGE+Trinity+SOUL+L0 gated).
    pub fn build_system_prompt(&self) -> String {
        crate::cognitive_bridge::cortex_system_prompt("")
    }

    // NOTE (freeze s330): `build_system_prompt_for` foi removido — ele chamava
    // `cortex_system_prompt` (que re-locka SKILL_STORAGE) e, por ser um método
    // `&self`, só era utilizável com o guard do TicketLock em mão → self-deadlock
    // não-reentrante. O prompt seguro é montado no caller (agents.rs) sem segurar
    // o lock durante `cortex_system_prompt`.
}

/// Invalida o índice de skills — o próximo prompt reconstrói (consumido pelo
/// CHANGE_NOTIFY lane: skill mudou sob o loader).
pub fn invalidate_skill_index() {
    SKILLS_INDEXED.store(false, Ordering::Relaxed);
}

/// Estado do reload boot→Runtime (Q2): o boot chama `reload_persisted_wasm_skills`
/// antes de existir mount `/skills` → SKIP armado; o retry tenta 1× quando o
/// VFS aparece e marca DONE mesmo se 0 skills (nunca busy-loop, sem log/tick).
static RELOAD_DONE: AtomicBool = AtomicBool::new(false);
static RELOAD_SKIPPED: AtomicBool = AtomicBool::new(false);
/// Orcamento do retry (F1.5): 1x so. Sem isto a cauda rearmava SKIPPED a cada
/// tick enquanto o Tickv nao montasse, porque `retry_reload_if_skipped` grava
/// DONE=true ANTES de chamar o reload. O agente que chama o retry a cada tick
/// (SelfEvolveAgent) virava busy-loop no proprio dono do ciclo de vida das skills.
static RELOAD_RETRIED: AtomicBool = AtomicBool::new(false);
/// `true` quando o ULTIMO reload NAO executou o pass duravel (Tickv ausente).
/// UNKNOWN != 0: `durable=0` sozinho nao distingue "zero skills duraveis" de
/// "nao olhei" — e o boot 2 do harness F1.5 lia esse 0 como FALSIFIED legitimo.
static RELOAD_DURABLE_UNKNOWN: AtomicBool = AtomicBool::new(false);

/// Garante o mount `/skills` → ramfs (idempotente, silencioso). Root cause Q2:
/// `init_standard_mounts` monta /mnt/* mas nunca /skills → `resolve("/skills")`
/// caía no fallback sem agente e o reload SKIPava em TODO boot (não só cedo).
/// Mesma montagem que os testes já faziam; RamFsAgent vem do `init_fs_agents`.
fn ensure_skills_mount() {
    let mut guard = crate::vfs::VFS.lock();
    if let Some(ref mut v) = *guard {
        if !v.mount_table().iter().any(|m| m.mount_point == "/skills") {
            v.mount("/skills", "ramfs");
        }
    }
}

/// Retry 1× do reload quando o boot SKIPou (Q2). Engancha num tick Runtime já
/// existente (SelfEvolveAgent — dono do ciclo de vida das skills): no-op após
/// DONE ou sem SKIP armado; se o VFS ainda não está pronto segue armado e
/// silencioso (sem log por tick). O resumo sai do reload normal.
pub fn retry_reload_if_skipped() {
    if RELOAD_DONE.load(Ordering::Relaxed) || !RELOAD_SKIPPED.load(Ordering::Relaxed) {
        return;
    }
    if !crate::fs::vfs_ready_for_wasm() {
        return;
    }
    RELOAD_SKIPPED.store(false, Ordering::Relaxed);
    RELOAD_DONE.store(true, Ordering::Relaxed); // 1× só — mesmo se 0 skills/SKIP.
    RELOAD_RETRIED.store(true, Ordering::Relaxed); // consome o orcamento
    reload_persisted_wasm_skills();
}

/// Boot hook (in-hermes): re-registra `/skills/*.wasm` persistidos no VFS
/// como WasmSkill no sandbox wasmi (Caminho A) — recarregadas EXECUTAM
/// (unifica com o promote; DynamicSkill com `wasm` sem bridge era stub).
/// Best-effort: VFS ausente segue o pass Tickv; bytes que falham no sandbox/register
/// são pulados com log (nunca panic). B3: o sidecar `/skills/{name}.prov`
/// devolve a proveniência ORIGINAL; ausente (skill antiga) = `Reloaded` + warn.
pub fn reload_persisted_wasm_skills() -> u32 {
    // ponytail: mount tardio — sem ele o list falha em todo boot (Q2).
    ensure_skills_mount();
    let items = match crate::fs::list_vfs("/skills") {
        Ok(v) => v,
        Err(_) => {
            k_nano::slog_hermes!("SKILL", "warn", "reload VFS absent — segue tickv");
            alloc::vec::Vec::new()
        }
    };
    let mut n = 0u32;
    let mut c_born = 0u32;
    let mut c_tmpl = 0u32;
    let mut c_dummy = 0u32;
    let mut c_imp = 0u32;
    let mut c_rel = 0u32;
    for item in &items {
        if !item.ends_with(".wasm") {
            continue;
        }
        let path = alloc::format!("/skills/{}", item.trim_start_matches('/'));
        let bytes = match crate::fs::read_vfs(&path) {
            Ok(b) => b,
            Err(_) => {
                k_nano::slog_hermes!("SKILL", "warn", "reload SKIP {} (read fail)", path);
                continue;
            }
        };
        if !crate::wasmi_rt::sandbox_validate_and_run(&bytes) {
            k_nano::slog_hermes!("SKILL", "warn", "reload SKIP {} (sandbox fail)", path);
            continue;
        }
        let name = item
            .trim_start_matches('/')
            .strip_suffix(".wasm")
            .unwrap_or(item);
        let prov = match crate::wasmi_rt::read_provenance_sidecar(name) {
            Some(p) => p,
            None => {
                k_nano::slog_hermes!("SKILL", "warn", "reload {} sem sidecar .prov → prov=reloaded", path);
                crate::wasmi_rt::SkillProvenance::Reloaded
            }
        };
        match crate::wasmi_rt::register_wasm_skill_with_provenance(
            &bytes,
            name,
            "reloaded /skills/*.wasm",
            prov,
        ) {
            Ok(()) => {
                n = n.saturating_add(1);
                crate::wasmi_rt::note_reload_ok();
                match prov {
                    crate::wasmi_rt::SkillProvenance::ModelBorn => c_born += 1,
                    crate::wasmi_rt::SkillProvenance::Template => c_tmpl += 1,
                    crate::wasmi_rt::SkillProvenance::Dummy => c_dummy += 1,
                    crate::wasmi_rt::SkillProvenance::Imported => c_imp += 1,
                    crate::wasmi_rt::SkillProvenance::Reloaded => c_rel += 1,
                }
            }
            Err(e) => {
                k_nano::slog_hermes!("SKILL", "warn", "reload SKIP {} (register: {})", path, e);
                continue;
            }
        }
    }
    // F1.5 (OPCODE-0042 Option 2): fonte DURÁVEL TickvLite — o ramfs /skills é
    // volátil e some no reboot. Enumera `skill/wasm/` e registra o que não veio
    // do VFS; proveniência de `skill/wasm_prov/{name}`. Best-effort (sem tickv
    // = 0 + silêncio: não é erro de boot). NÃO segura o lock do tickv durante
    // sandbox/register (get_blob pega e solta).
    let mut c_durable = 0u32;
    let mut durable_unknown = false;
    let durable_keys: Vec<String> =
        match k_nano::storage::with_tickv(|kv| kv.keys_with_prefix("skill/wasm/")) {
            Some(v) => v,
            None => {
                // `with_tickv` devolve None quando o Tickv nao esta montado — que e
                // o caso normal do reload do BOOT (main.rs chama antes do mount).
                // `unwrap_or_default` achatava isso em Vec vazio e a cauda marcava
                // DONE: o boot registava "pronto" tendo feito um pass que nao rodou.
                durable_unknown = true;
                Vec::new()
            }
        };
    for key in &durable_keys {
        let name = key.strip_prefix("skill/wasm/").unwrap_or(key.as_str());
        if name.is_empty() || crate::globals::SKILL_REGISTRY.lock().has_skill(name) {
            continue; // já registrada pelo VFS
        }
        let bytes = match k_nano::storage::get_blob(key) {
            Ok(b) => b,
            Err(_) => {
                k_nano::slog_hermes!("SKILL", "warn", "reload durable SKIP {} (get fail)", key);
                continue;
            }
        };
        if !crate::wasmi_rt::sandbox_validate_and_run(&bytes) {
            k_nano::slog_hermes!("SKILL", "warn", "reload durable SKIP {} (sandbox fail)", key);
            continue;
        }
        let pkey = alloc::format!("skill/wasm_prov/{}", name);
        let prov = match k_nano::storage::get_blob(&pkey)
            .ok()
            .as_deref()
            .and_then(crate::wasmi_rt::parse_provenance_bytes)
        {
            Some(p) => p,
            None => {
                k_nano::slog_hermes!(
                    "SKILL",
                    "warn",
                    "reload durable {} sem prov → prov=reloaded",
                    key
                );
                crate::wasmi_rt::SkillProvenance::Reloaded
            }
        };
        match crate::wasmi_rt::register_wasm_skill_with_provenance(
            &bytes,
            name,
            "reloaded tickv skill/wasm/*",
            prov,
        ) {
            Ok(()) => {
                n = n.saturating_add(1);
                c_durable += 1;
                crate::wasmi_rt::note_reload_ok();
                match prov {
                    crate::wasmi_rt::SkillProvenance::ModelBorn => c_born += 1,
                    crate::wasmi_rt::SkillProvenance::Template => c_tmpl += 1,
                    crate::wasmi_rt::SkillProvenance::Dummy => c_dummy += 1,
                    crate::wasmi_rt::SkillProvenance::Imported => c_imp += 1,
                    crate::wasmi_rt::SkillProvenance::Reloaded => c_rel += 1,
                }
            }
            Err(e) => {
                k_nano::slog_hermes!(
                    "SKILL",
                    "warn",
                    "reload durable SKIP {} (register: {})",
                    key,
                    e
                );
            }
        }
    }
    // `durable=` continua no prefixo (o harness F1.5 faz grep nessa linha, em
    // ORACLE-0055) e o estado vira um campo a mais: 0 com durable_unknown=true e
    // "UNKNOWN, nao olhei", nao "zero skills".
    k_nano::slog_hermes!("SKILL", "ok", "[skills][ok] reload n={} model-born={} template={} dummy={} imported={} reloaded={} durable={} durable_unknown={}", n, c_born, c_tmpl, c_dummy, c_imp, c_rel, c_durable, durable_unknown);
    RELOAD_DURABLE_UNKNOWN.store(durable_unknown, Ordering::Relaxed);
    if durable_unknown {
        k_nano::slog_hermes!("SKILL", "warn", "[skills][skip] durable tickv ausente — pass NAO executado (UNKNOWN != 0)");
    }
    let (done, skipped) = reload_tail_flags(!durable_unknown, RELOAD_RETRIED.load(Ordering::Relaxed));
    RELOAD_DONE.store(done, Ordering::Relaxed);
    RELOAD_SKIPPED.store(skipped, Ordering::Relaxed);
    n
}

/// `true` quando o ultimo reload NAO rodou o pass duravel (Tickv ausente).
/// Superficie honesta para o watchdog e para o harness: o resumo `durable=0`
/// sozinho e ilegivel como veredito.
pub fn reload_durable_unknown() -> bool {
    RELOAD_DURABLE_UNKNOWN.load(Ordering::Relaxed)
}

/// Decisao da CAUDA do reload como funcao PURA — (RELOAD_DONE, RELOAD_SKIPPED).
///
/// Fica pura de proposito: a variante integrada depende de o Tickv estar montado,
/// e na suite de host isso depende da ORDEM dos testes (o teste que monta o Tickv
/// roda antes do que checa o reload). Um teste que so pega um dos lados nao prova
/// o contrato; aqui os 3 casos sao verificaveis sem nenhum estado global.
///
/// Contrato:
/// - pass rodou        -> (true, false): pronto, nada a repetir;
/// - pass NAO rodou e
///   orcamento sobrando -> (false, true): nao marca DONE (UNKNOWN != 0) e rearma o
///                        retry, porque o Tickv pode montar depois (o reload do
///                        BOOT roda antes do mount);
/// - pass NAO rodou e
///   orcamento gasto    -> (true, false): o wrapper ja gravou DONE; so nao rearma
///                        (rearmar aqui = reentrada todo tick no SelfEvolveAgent).
pub(crate) fn reload_tail_flags(durable_ran: bool, retried: bool) -> (bool, bool) {
    match (durable_ran, retried) {
        (true, _) => (true, false),
        (false, true) => (true, false),
        (false, false) => (false, true),
    }
}

pub fn load_embedded_skills() -> SkillLoader {
    let mut loader = SkillLoader::new();

    // Skills embutidas via include_str! (path relativo ao workspace root)
    let skills_raw: [&str; 4] = [
        include_str!("../../../skills/hw_identify/SKILL.md"),
        include_str!("../../../skills/self_heal/SKILL.md"),
        include_str!("../../../skills/web_scrape/SKILL.md"),
        include_str!("../../../skills/skill_writer/SKILL.md"),
    ];

    for content in &skills_raw {
        if let Err(e) = loader.register_trusted_skill(content) {
            k_nano::slog_hermes!("SKILL", "info", "Erro ao carregar skill: {}", e);
        }
    }

    let count = loader.skills.len();
    let system = loader.build_system_prompt();
    k_nano::slog_hermes!("SKILL", "info", "{} skill(s) carregadas, prompt de {} bytes", count, system.len());
    loader
}

#[cfg(test)]
mod lane_b_tests {
    use super::*;

    /// VFS de teste: mount `/skills` → ramfs (idempotente, compartilha o
    /// STORE global do RamFsAgent — nomes únicos por teste, sem teardown).
    fn setup_test_vfs() {
        {
            let mut guard = crate::vfs::VFS.lock();
            if guard.is_none() {
                *guard = Some(crate::vfs::VfsRegistry::new());
            }
            if let Some(ref mut v) = *guard {
                if !v.mount_table().iter().any(|m| m.mount_point == "/skills") {
                    v.mount("/skills", "ramfs");
                }
            }
        }
        if crate::fs::FS_AGENTS.lock().is_empty() {
            crate::fs::register_fs_agent(alloc::boxed::Box::new(
                crate::fs::ram_fs_agent::RamFsAgent::new(),
            ));
        }
    }

    #[test]
    fn model_born_round_trip_persist_reload_executes() {
        setup_test_vfs();
        let name = "lb_rt_skill";
        // 1. promote model-born persiste /skills/{name}.wasm + .prov no VFS.
        assert!(crate::evolve::promote_model_text_to_wasm(name, "round-trip", "a*2+1").is_ok());
        assert_eq!(
            crate::wasmi_rt::skill_provenance(name),
            Some(crate::wasmi_rt::SkillProvenance::ModelBorn)
        );
        let persisted =
            crate::fs::read_vfs("/skills/lb_rt_skill.wasm").expect("wasm persistido");
        assert_eq!(&persisted[0..4], &[0x00, 0x61, 0x73, 0x6D]);
        let sidecar =
            crate::fs::read_vfs("/skills/lb_rt_skill.prov").expect("sidecar persistido");
        assert_eq!(&sidecar, b"model-born");
        // 2. drop do registry simula reboot; reload recupera o carimbo ORIGINAL.
        assert!(crate::globals::SKILL_REGISTRY.lock().unregister(name));
        assert!(!crate::globals::SKILL_REGISTRY.lock().has_skill(name));
        let rel_before = crate::wasmi_rt::metrics_reload_ok();
        let n = reload_persisted_wasm_skills();
        assert!(n >= 1);
        assert!(crate::wasmi_rt::metrics_reload_ok() >= rel_before + 1);
        assert!(crate::globals::SKILL_REGISTRY.lock().has_skill(name));
        assert_eq!(
            crate::wasmi_rt::skill_provenance(name),
            Some(crate::wasmi_rt::SkillProvenance::ModelBorn)
        );
        // 3. recarregada executa no wasmi: a*2+1 com payload "6" → 13.
        {
            let mut reg = crate::globals::SKILL_REGISTRY.lock();
            reg.set_policy(
                name,
                skill_registry::ToolPolicy { enabled: true, auto_approve: true },
            );
        }
        let out = crate::globals::SKILL_REGISTRY
            .lock()
            .execute_skill_unchecked(name, b"6")
            .expect("reload deve executar no wasmi");
        let text = core::str::from_utf8(&out).expect("utf8");
        assert!(text.contains("13"), "esperava a*2+1=13, veio {}", text);
        crate::globals::SKILL_REGISTRY.lock().unregister(name);
    }

    /// F1.5 (OPCODE-0054): variante que ESVAZIA o ramfs volátil (simula reboot)
    /// e prova que o reload recupera via TickvLite durável. Requer
    /// `--test-threads=1` (STORE/FLASH globais compartilhados).
    #[test]
    fn model_born_round_trip_reloads_from_durable_tickv_after_ramfs_clear() {
        setup_test_vfs();
        // TickvLite limpo (RamFlash) para o teste ser determinístico.
        *k_nano::storage::TICKV.lock() = None;
        *k_nano::storage::FLASH.lock() = None;
        k_nano::storage::install_ram_flash(256 * 1024);

        let name = "lb_durable_skill";
        // 1. promote persiste no VFS *e* no tickv durável.
        assert!(crate::evolve::promote_model_text_to_wasm(name, "durable", "a*2+1").is_ok());
        // 2. reboot simulado: ramfs volátil esvazia; tickv é a fonte durável.
        crate::fs::ram_fs_agent::store().clear();
        assert!(crate::fs::read_vfs("/skills/lb_durable_skill.wasm").is_err());
        // 3. registry limpo (reboot) + reload → recupera do tickv.
        assert!(crate::globals::SKILL_REGISTRY.lock().unregister(name));
        assert!(!crate::globals::SKILL_REGISTRY.lock().has_skill(name));
        let rel_before = crate::wasmi_rt::metrics_reload_ok();
        let n = reload_persisted_wasm_skills();
        assert!(n >= 1, "reload deveria vir do tickv durável");
        assert!(crate::wasmi_rt::metrics_reload_ok() >= rel_before + 1);
        assert!(crate::globals::SKILL_REGISTRY.lock().has_skill(name));
        assert_eq!(
            crate::wasmi_rt::skill_provenance(name),
            Some(crate::wasmi_rt::SkillProvenance::ModelBorn)
        );
        // 4. recarregada executa no wasmi: a*2+1 com "6" → 13.
        {
            let mut reg = crate::globals::SKILL_REGISTRY.lock();
            reg.set_policy(
                name,
                skill_registry::ToolPolicy { enabled: true, auto_approve: true },
            );
        }
        let out = crate::globals::SKILL_REGISTRY
            .lock()
            .execute_skill_unchecked(name, b"6")
            .expect("reload durável deve executar no wasmi");
        let text = core::str::from_utf8(&out).expect("utf8");
        assert!(text.contains("13"), "esperava a*2+1=13, veio {}", text);
        crate::globals::SKILL_REGISTRY.lock().unregister(name);
    }

    #[test]
    fn reload_without_sidecar_falls_back_to_reloaded() {
        setup_test_vfs();
        // Skill antiga: .wasm direto no VFS, sem .prov.
        let (n_params, ops) = crate::wasm_build::model_text_to_ops("a+1").expect("ops");
        let wasm = crate::wasm_build::build_run_module(n_params, &ops).expect("build");
        crate::fs::write_vfs("/skills/lb_old_skill.wasm", &wasm).expect("write");
        let _ = crate::globals::SKILL_REGISTRY.lock().unregister("lb_old_skill");
        let n = reload_persisted_wasm_skills();
        assert!(n >= 1);
        assert!(crate::globals::SKILL_REGISTRY.lock().has_skill("lb_old_skill"));
        assert_eq!(
            crate::wasmi_rt::skill_provenance("lb_old_skill"),
            Some(crate::wasmi_rt::SkillProvenance::Reloaded)
        );
        crate::globals::SKILL_REGISTRY.lock().unregister("lb_old_skill");
    }

    #[test]
    fn skipped_boot_reload_retries_once_in_runtime() {
        use core::sync::atomic::Ordering;
        setup_test_vfs();
        let name = "lb_retry_skill";
        // Arrange: skill persistida (wasm + sidecar) mas registry limpo,
        // com flags simulando o boot que SKIPou antes do VFS existir.
        assert!(crate::evolve::promote_model_text_to_wasm(name, "retry-desc", "a*2+1").is_ok());
        assert!(crate::globals::SKILL_REGISTRY.lock().unregister(name));
        assert!(!crate::globals::SKILL_REGISTRY.lock().has_skill(name));
        super::RELOAD_DONE.store(false, Ordering::Relaxed);
        super::RELOAD_SKIPPED.store(true, Ordering::Relaxed);
        // Act: tick Runtime engancha o retry → recarrega 1× com prov original.
        let rel_before = crate::wasmi_rt::metrics_reload_ok();
        super::retry_reload_if_skipped();
        assert!(crate::globals::SKILL_REGISTRY.lock().has_skill(name));
        assert_eq!(
            crate::wasmi_rt::skill_provenance(name),
            Some(crate::wasmi_rt::SkillProvenance::ModelBorn)
        );
        assert!(crate::wasmi_rt::metrics_reload_ok() >= rel_before + 1);
        // 2ª chamada = no-op (DONE): sem re-reload, sem log extra.
        let rel_after = crate::wasmi_rt::metrics_reload_ok();
        super::retry_reload_if_skipped();
        assert_eq!(crate::wasmi_rt::metrics_reload_ok(), rel_after);
        crate::globals::SKILL_REGISTRY.lock().unregister(name);
        // Estado pós-boot canônico p/ os demais testes (DONE, sem SKIP armado).
        super::RELOAD_RETRIED.store(false, Ordering::Relaxed);
    }

    /// UNKNOWN != 0 (F1.5): se o Tickv nao esta montado, o pass duravel NAO
    /// rodou — e o estado tem de dizer isso. Antes do patch a cauda marcava
    /// DONE=true e SKIPPED=false incondicionalmente, entao o boot 2 lia
    /// `durable=0` como se fosse "zero skills duraveis" e o harness accusava
    /// FALSIFIED na camada errada.
    #[test]
    fn durable_unknown_is_not_reported_as_zero() {
        setup_test_vfs();
        let tickv_mounted = k_nano::storage::with_tickv(|_| ()).is_some();
        super::RELOAD_RETRIED.store(false, Ordering::Relaxed);
        super::RELOAD_DONE.store(false, Ordering::Relaxed);
        super::RELOAD_SKIPPED.store(false, Ordering::Relaxed);

        let _ = super::reload_persisted_wasm_skills();

        assert_eq!(
            super::reload_durable_unknown(),
            !tickv_mounted,
            "o pass duravel tem de estar UNKNOWN quando nao ha Tickv montado"
        );
        if tickv_mounted {
            assert!(super::RELOAD_DONE.load(Ordering::Relaxed));
            assert!(!super::RELOAD_SKIPPED.load(Ordering::Relaxed));
        } else {
            // Pass nao rodou => o boot nao pode estar DONE, e o retry fica
            // armado para quando o Tickv montar (main.rs chama antes do mount).
            assert!(
                !super::RELOAD_DONE.load(Ordering::Relaxed),
                "DONE com pass duravel nao executado = estado mentindo"
            );
            assert!(
                super::RELOAD_SKIPPED.load(Ordering::Relaxed),
                "sem Tickv o retry tem de ficar armado"
            );
        }
        // canonico p/ os demais testes
        super::RELOAD_RETRIED.store(false, Ordering::Relaxed);
        super::RELOAD_SKIPPED.store(false, Ordering::Relaxed);
        super::RELOAD_DONE.store(true, Ordering::Relaxed);
    }

    /// Orcamento de 1x: o retry consome o orcamento e nao reentra. Sem isto a
    /// cauda rearmava SKIPPED a cada tick (o wrapper grava DONE=true antes de
    /// chamar o reload) e o agente que chama o retry todo tick virava busy-loop.
    ///
    /// O CONTRATO fica em `reload_tail_flags` (puro) — verificado nos 3 casos
    /// sem depender do Tickv nem da ordem dos testes. Aqui so o que a integracao
    /// garante independente de ramo: 1a chamada consome o orcamento, 2a e no-op.
    #[test]
    fn retry_budget_is_one_shot() {
        // Contrato puro: 3 casos, nenhum global.
        assert_eq!(super::reload_tail_flags(true, false), (true, false), "pass rodou: DONE");
        assert_eq!(super::reload_tail_flags(true, true), (true, false), "pass rodou: DONE");
        assert_eq!(
            super::reload_tail_flags(false, false),
            (false, true),
            "UNKNOWN com orcamento sobrando: nao DONE e rearma"
        );
        assert_eq!(
            super::reload_tail_flags(false, true),
            (true, false),
            "UNKNOWN com orcamento gasto: nao rearma (senao reentra todo tick)"
        );

        setup_test_vfs();
        super::RELOAD_RETRIED.store(false, Ordering::Relaxed);
        super::RELOAD_DONE.store(false, Ordering::Relaxed);
        super::RELOAD_SKIPPED.store(true, Ordering::Relaxed);

        super::retry_reload_if_skipped();
        assert!(
            super::RELOAD_RETRIED.load(Ordering::Relaxed),
            "a 1a tentativa precisa consumir o orcamento"
        );

        // 2a chamada = no-op (ja DONE).
        let before = crate::wasmi_rt::metrics_reload_ok();
        super::retry_reload_if_skipped();
        assert_eq!(crate::wasmi_rt::metrics_reload_ok(), before);
        // canonico p/ os demais testes
        super::RELOAD_RETRIED.store(false, Ordering::Relaxed);
        super::RELOAD_SKIPPED.store(false, Ordering::Relaxed);
        super::RELOAD_DONE.store(true, Ordering::Relaxed);
    }
}






