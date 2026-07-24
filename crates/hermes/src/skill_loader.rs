use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
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

    /// Parse a skill markdown file, validate security, and add to registry
    pub fn register_skill(&mut self, content: &str) -> Result<(), &'static str> {
        // Sprint 108: verificação estrutural antes do parse completo
        if let crate::self_evolve::VerifyVerdict::Reject(reason) =
            crate::self_evolve::verify_skill_md(content)
        {
            k_nano::slog_hermes!("SKILL", "VERIFY", "REJECT: {}", reason);
            return Err(reason);
        }
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

    /// Build system prompt — cognitive bridge (BGE+Trinity+SOUL+L0 gated).
    pub fn build_system_prompt(&self) -> String {
        crate::cognitive_bridge::cortex_system_prompt("")
    }

    /// Prompt contextualizado com intent do usuário.
    pub fn build_system_prompt_for(&self, intent: &str) -> String {
        crate::cognitive_bridge::cortex_system_prompt(intent)
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
        if let Err(e) = loader.register_skill(content) {
            k_nano::slog_hermes!("SKILL", "info", "Erro ao carregar skill: {}", e);
        }
    }

    let count = loader.skills.len();
    let system = loader.build_system_prompt();
    k_nano::slog_hermes!("SKILL", "info", "{} skill(s) carregadas, prompt de {} bytes", count, system.len());
    loader
}
