//! Matrix Learning Pipeline — IDEA #311f
//!
//! On-demand skill acquisition: "quero pilotar helicóptero" → domain →
//! SKILL.md → registered skill → immediately usable.
//!
//! Based on the Trinity model from The Matrix — download ANY knowledge
//! and use it as a skill. Pragmatic: generates SKILL.md templates for
//! identified domains and registers them via the existing skill pipeline.
//! Upgrade path: use Cortex LLM to generate richer skill content.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use crate::globals::{EVENT_BUS, SKILL_STORAGE};

// ---------------------------------------------------------------------------
// OnDemandLearning — the core pipeline
// ---------------------------------------------------------------------------

/// The Matrix Learning pipeline.
///
/// 1. Parse learning intent → knowledge domain
/// 2. Generate SKILL.md for that domain
/// 3. Register skill → immediately available
pub struct OnDemandLearning {
    /// Whether the pipeline is active. Currently always true;
    /// gate for future on/off control.
    pub active: bool,
}

impl OnDemandLearning {
    pub fn new() -> Self {
        OnDemandLearning { active: true }
    }

    /// Handle a learning request end-to-end.
    ///
    /// Pipeline: parse intent → identify knowledge domain →
    /// generate skill template → register skill.
    pub fn handle_learning_request(&mut self, text: &str) -> Result<String, &'static str> {
        if !self.active {
            return Err("pipeline inativo");
        }
        let domain = self.parse_learning_intent(text)?;
        let skill_code = self.generate_skill_from_domain(&domain)?;
        let skill_name = alloc::format!("learned_{}", domain);
        self.register_learned_skill(&skill_name, &skill_code)?;
        k_nano::slog_hermes!(
            "MATRIX",
            "LEARN",
            "domain={} skill={} — pipeline OK",
            domain,
            skill_name
        );
        Ok(alloc::format!(
            "Aprendi {}! Skill '{}' registrada.",
            domain,
            skill_name
        ))
    }

    /// Parse natural language learning request into a knowledge domain slug.
    fn parse_learning_intent(&self, text: &str) -> Result<String, &'static str> {
        let lower = text.to_ascii_lowercase();
        if lower.contains("pilotar")
            || lower.contains("voar")
            || lower.contains("helicoptero")
            || lower.contains("helicopter")
        {
            Ok("helicopter_pilot".into())
        } else if lower.contains("cozinhar")
            || lower.contains("receita")
            || lower.contains("culinaria")
            || lower.contains("cook")
        {
            Ok("cooking".into())
        } else if lower.contains("programar")
            || lower.contains("codigo")
            || lower.contains("coding")
            || lower.contains("programming")
            || lower.contains("rust")
            || lower.contains("code")
        {
            Ok("coding".into())
        } else if lower.contains("tocar")
            || lower.contains("musica")
            || lower.contains("instrumento")
            || lower.contains("music")
            || lower.contains("play")
        {
            Ok("music".into())
        } else if lower.contains("falar")
            || lower.contains("idioma")
            || lower.contains("lingua")
            || lower.contains("language")
            || lower.contains("speak")
        {
            Ok("language".into())
        } else if lower.contains("heal")
            || lower.contains("curar")
            || lower.contains("medicina")
            || lower.contains("medical")
            || lower.contains("primeiros socorros")
        {
            Ok("medical".into())
        } else if lower.contains("lutar")
            || lower.contains("fight")
            || lower.contains("kung")
            || lower.contains("judo")
            || lower.contains("jiu")
            || lower.contains("martial")
        {
            Ok("martial_arts".into())
        } else if lower.contains("pilot")
            || lower.contains("plane")
            || lower.contains("aviao")
            || lower.contains("aeronave")
        {
            Ok("aviation".into())
        } else if lower.contains("mechanic")
            || lower.contains("mecanica")
            || lower.contains("motor")
            || lower.contains("engine")
        {
            Ok("mechanics".into())
        } else if lower.contains("art")
            || lower.contains("desenhar")
            || lower.contains("pintar")
            || lower.contains("draw")
        {
            Ok("art".into())
        } else if lower.contains("crypto")
            || lower.contains("bitcoin")
            || lower.contains("blockchain")
        {
            Ok("crypto".into())
        } else {
            Ok("general_knowledge".into())
        }
    }

    /// Generate a SKILL.md based on the domain.
    ///
    /// ponytail: template-based generation. Upgrade to Cortex LLM generation
    /// when a model is loaded and the user wants richer content.
    fn generate_skill_from_domain(&self, domain: &str) -> Result<String, &'static str> {
        let (description, steps, preflight) = match domain {
            "helicopter_pilot" => (
                "Pilotar helicóptero — conhecimentos essenciais de voo",
                vec![
                    "1. Verificar pré-voo: combustível, instrumentos, controles",
                    "2. Iniciar turbina: throttle idle, rotor clutch engage",
                    "3. Coletivo: aumentar gradualmente para decolagem",
                    "4. Pedais antitorque: compensar torque do rotor principal",
                    "5. Cíclico: controlar atitude e direção",
                    "6. Navegação: seguir checkpoints e altimetria",
                    "7. Aterrissagem: reduzir coletivo, flare suave",
                ],
                vec![
                    "- [ ] Pré-voo completo",
                    "- [ ] Briefing de rota",
                    "- [ ] Rádio check",
                ],
            ),
            "cooking" => (
                "Culinária — receitas e técnicas",
                vec![
                    "1. Selecionar receita e ingredientes necessários",
                    "2. Preparar estação de trabalho (mise en place)",
                    "3. Seguir técnica culinária: corte, temperatura, tempo",
                    "4. Degustar e ajustar temperos",
                    "5. Emplatar e servir",
                ],
                vec![
                    "- [ ] Ingredientes separados e medidos",
                    "- [ ] Utensílios limpos e prontos",
                ],
            ),
            "coding" => (
                "Programação de computadores — fundamentos e práticas",
                vec![
                    "1. Definir o problema e requisitos",
                    "2. Escolher linguagem e ferramentas apropriadas",
                    "3. Escrever código limpo e modular",
                    "4. Testar cada unidade funcional",
                    "5. Revisar e refatorar",
                ],
                vec![
                    "- [ ] Ambiente configurado",
                    "- [ ] Linter / formatador aplicado",
                    "- [ ] Testes passando",
                ],
            ),
            "music" => (
                "Teoria e prática musical",
                vec![
                    "1. Aquecimento e postura correta",
                    "2. Escalas e exercícios técnicos",
                    "3. Praticar repertório escolhido",
                    "4. Gravar e autoavaliar",
                ],
                vec![
                    "- [ ] Instrumento afinado",
                    "- [ ] Partitura/tablatura disponível",
                ],
            ),
            "language" => (
                "Aprendizado de idiomas",
                vec![
                    "1. Escutar e repetir: fonética e pronúncia",
                    "2. Vocabulário: 10 novas palavras por sessão",
                    "3. Gramática: uma regra por vez",
                    "4. Conversação: praticar com nativos ou IA",
                    "5. Imersão: filmes, músicas, leitura",
                ],
                vec![
                    "- [ ] Material de estudo disponível",
                    "- [ ] Fone de ouvido para áudio",
                ],
            ),
            "medical" => (
                "Conhecimentos médicos e primeiros socorros",
                vec![
                    "1. Avaliar cena e segurança",
                    "2. Verificar responsividade e chamar ajuda",
                    "3. Verificar respiração e pulso",
                    "4. Aplicar primeiros socorros conforme protocolo",
                    "5. Monitorar sinais vitais até ajuda chegar",
                ],
                vec![
                    "- [ ] Kit de primeiros socorros disponível",
                    "- [ ] Número de emergência visível",
                ],
            ),
            "martial_arts" => (
                "Artes marciais — fundamentos de defesa pessoal",
                vec![
                    "1. Posição base: equilíbrio e guarda",
                    "2. Deslocamento: footwork e angulação",
                    "3. Defesa: bloqueios e esquivas",
                    "4. Ataque: golpes básicos combinados",
                    "5. Condicionamento: resistência e flexibilidade",
                ],
                vec![
                    "- [ ] Espaço seguro e livre",
                    "- [ ] Equipamento de proteção básico",
                ],
            ),
            "aviation" => (
                "Aviação — pilotagem de aeronaves",
                vec![
                    "1. Inspeção pré-voo: exteriores e interiores",
                    "2. Checklist de partida do motor",
                    "3. Taxi e autorização de tráfego",
                    "4. Decolagem e subida inicial",
                    "5. Navegação por instrumentos e visuais",
                    "6. Aproximação e pouso",
                    "7. Desligamento e pós-voo",
                ],
                vec![
                    "- [ ] Briefing meteorológico",
                    "- [ ] Plano de voo arquivado",
                    "- [ ] Combustível suficiente com reserva",
                ],
            ),
            "mechanics" => (
                "Mecânica automotiva — manutenção e reparos",
                vec![
                    "1. Diagnosticar sintoma: barulho, vibração, falha",
                    "2. Consultar manual de serviço do veículo",
                    "3. Reunir ferramentas e peças necessárias",
                    "4. Executar reparo seguindo procedimento",
                    "5. Testar e verificar funcionamento",
                ],
                vec![
                    "- [ ] Ferramentas adequadas disponíveis",
                    "- [ ] EPI: luvas, óculos de proteção",
                ],
            ),
            "art" => (
                "Artes visuais — desenho, pintura e composição",
                vec![
                    "1. Esboçar composição: linhas gerais e proporção",
                    "2. Refinar detalhes e contornos",
                    "3. Aplicar cores e sombreamento",
                    "4. Revisar equilíbrio e contraste",
                    "5. Finalizar e proteger a obra",
                ],
                vec![
                    "- [ ] Materiais separados (papel, tinta, pincéis)",
                    "- [ ] Referência visual disponível",
                ],
            ),
            "crypto" => (
                "Criptomoedas e blockchain — fundamentos",
                vec![
                    "1. Entender conceito: ledger distribuído, consenso, mineração",
                    "2. Escolher carteira: hot wallet vs cold storage",
                    "3. Adquirir criptomoeda em exchange confiável",
                    "4. Segurança: chave privada, 2FA, backup",
                    "5. Acompanhar mercado e notícias regulatórias",
                ],
                vec![
                    "- [ ] Carteira configurada e testada",
                    "- [ ] Backup da seed phrase armazenado offline",
                ],
            ),
            _ => (
                "Conhecimento geral — guia de aprendizado rápido",
                vec![
                    "1. Identificar tópico e recursos disponíveis",
                    "2. Pesquisar fontes confiáveis (livros, cursos, docs)",
                    "3. Praticar conceitos com exercícios",
                    "4. Avaliar compreensão e revisar pontos fracos",
                    "5. Aplicar conhecimento em projeto prático",
                ],
                vec![
                    "- [ ] Fonte de pesquisa disponível",
                    "- [ ] Tempo reservado para estudo",
                ],
            ),
        };

        let skill = format!(
            "---\nname: learned_{}\ndescription: {}\nrequired_tokens: [1]\n---\n\n\
             ## Workflow\n{}\n\n## Pre-Flight Verification\n{}\n",
            domain,
            description,
            steps.join("\n"),
            preflight.join("\n"),
        );
        Ok(skill)
    }

    /// Register the generated skill via the shared SkillLoader.
    fn register_learned_skill(&self, name: &str, content: &str) -> Result<(), &'static str> {
        let mut storage = SKILL_STORAGE.lock();
        // Remove previous version if re-learning
        storage.remove_skill(name);
        storage.register_skill(content)
    }
}

// ---------------------------------------------------------------------------
// Learning intent detection helper
// ---------------------------------------------------------------------------

/// Detect if a user message is a learning request.
///
/// Matches patterns like "quero aprender X", "aprender Y", "learn Z",
/// "download skill", "Matrix/Neo", and direct domain mentions.
pub fn is_learning_request(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    // Primary trigger: explicit learning intent
    if lower.contains("aprender")
        || lower.contains("quero aprender")
        || lower.contains("quero pilotar")
        || lower.contains("quero cozinhar")
        || lower.contains("quero programar")
        || (lower.contains("learn") && !lower.contains("machine learning"))
        || lower.contains("download") && lower.contains("skill")
    {
        return true;
    }
    // The Matrix reference triggers the pipeline
    if lower.contains("matrix") && (lower.contains("neo") || lower.contains("trinity") || lower.contains("download")) {
        return true;
    }
    // Direct domain knowledge acquisition
    if (lower.contains("ensinar") || lower.contains("teach") || lower.contains("aprenda"))
        && (lower.contains("skill") || lower.contains("skill") || lower.contains("como"))
    {
        return true;
    }
    // Domain-specific triggers (without explicit "aprender")
    if (lower.contains("helicopter") || lower.contains("pilotar") || lower.contains("voar"))
        && !lower.contains("simulador")
        && !lower.contains("jogo")
    {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// MatrixLearningAgent — event-driven learning agent
// ---------------------------------------------------------------------------

const MATRIX_LEARNING_MANIFEST: AgentManifest = AgentManifest {
    name: "matrix_learning",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(200),
    auto_start: true,
    persist: false,
};

/// Agent that listens for learning requests on USER_INTENT and routes them
/// through the OnDemandLearning pipeline.
///
/// Primary handler is HermesAgent::tick() integration (inline check before LLM
/// routing). This agent provides redundancy and future extensibility for
/// non-Chat learning request paths.
pub struct MatrixLearningAgent {
    receiver: Receiver,
    learner: OnDemandLearning,
}

impl MatrixLearningAgent {
    pub fn new() -> Self {
        MatrixLearningAgent {
            receiver: EVENT_BUS.subscribe("USER_INTENT"),
            learner: OnDemandLearning::new(),
        }
    }
}

impl Agent for MatrixLearningAgent {
    fn manifest(&self) -> &AgentManifest {
        &MATRIX_LEARNING_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if let Some(event) = self.receiver.try_receive() {
            if let Ok(text) = core::str::from_utf8(&event.payload) {
                if is_learning_request(text) {
                    match self.learner.handle_learning_request(text) {
                        Ok(result) => {
                            k_nano::slog_hermes!("MATRIX", "AGENT", "{}", result);
                            let _ = EVENT_BUS.publish(Event {
                                id: 0,
                                topic: String::from("HERMES_RESPONSE"),
                                payload: result.into_bytes(),
                                token: CapabilityToken::Legacy(1),
                            });
                        }
                        Err(e) => {
                            k_nano::slog_hermes!("MATRIX", "AGENT", "Erro: {}", e);
                            let _ = EVENT_BUS.publish(Event {
                                id: 0,
                                topic: String::from("HERMES_RESPONSE"),
                                payload: alloc::format!("[Matrix] Erro ao aprender: {}", e)
                                    .into_bytes(),
                                token: CapabilityToken::Legacy(1),
                            });
                        }
                    }
                }
            }
        }
        AgentTickResult::Pending
    }
}
