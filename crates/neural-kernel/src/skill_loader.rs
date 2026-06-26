use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use alloc::format;
use crate::serial_println;
use crate::cortex;

#[derive(Clone, Debug)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub required_tokens: Vec<u64>,
    pub instructions: String,
}

pub struct SkillLoader {
    pub skills: Vec<SkillManifest>,
}

impl SkillLoader {
    pub const fn new() -> Self {
        SkillLoader { skills: Vec::new() }
    }

    /// Parse a skill markdown file, validate security, and add to registry
    pub fn register_skill(&mut self, content: &str) -> Result<(), &'static str> {
        // Extract frontmatter (between --- markers)
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
        for line in frontmatter.lines() {
            if let Some(val) = line.strip_prefix("name: ") {
                name = val.trim();
            } else if let Some(val) = line.strip_prefix("description: ") {
                description = val.trim();
            } else if let Some(val) = line.strip_prefix("required_tokens: ") {
                tokens_str = val.trim();
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
                serial_println!("[SKILL-SEC] BLOQUEADO: skill '{}' contem padrao perigoso: '{}'", name, pattern);
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
        };

        serial_println!("[SKILL] Registrada: '{}' — {} ({} tokens, {} bytes)",
            manifest.name, manifest.description, tok_count, instructions.len());
        self.skills.push(manifest);
        Ok(())
    }

    /// Build a system prompt from all registered skills
    pub fn build_system_prompt(&self) -> String {
        let mut prompt = String::from("Voce e o Cortex LLM do Neural OS Hermes. Siga estas skills:\n\n");
        for skill in &self.skills {
            prompt.push_str(&format!("=== {} ===\n{}\n\n", skill.name, skill.instructions));
        }
        prompt.push_str("Responda de acordo com as instrucoes acima. Se nao houver instrucao relevante, use seu conhecimento geral treinado.\n");
        prompt
    }
}

pub fn load_embedded_skills() -> SkillLoader {
    let mut loader = SkillLoader::new();

    // Skills embutidas via include_str! (path relativo ao workspace root)
    let skills_raw: [&str; 2] = [
        include_str!("../../../skills/hw_identify.md"),
        include_str!("../../../skills/self_heal.md"),
    ];

    for content in &skills_raw {
        if let Err(e) = loader.register_skill(content) {
            serial_println!("[SKILL] Erro ao carregar skill: {}", e);
        }
    }

    let count = loader.skills.len();
    let system = loader.build_system_prompt();
    serial_println!("[SKILL] {} skill(s) carregadas, prompt de {} bytes", count, system.len());
    loader
}
