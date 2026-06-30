use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub name: String,
    pub division: String,
    pub mission: String,
    pub skills: Vec<String>,
    pub deliverable: String,
}

#[derive(Debug)]
pub struct Division {
    pub name: String,
    pub agents: Vec<AgentSpec>,
}

pub struct Agency {
    pub divisions: Vec<Division>,
}

impl Agency {
    pub fn new() -> Self {
        Agency { divisions: Self::build_divisions() }
    }

    fn build_divisions() -> Vec<Division> {
        let mut d = Vec::new();
        d.push(Division { name: String::from("engineering"), agents: vec![
            spec("system-architect", "engineering", "Projetar arquitetura do sistema", &["memory","pci","smp"]),
            spec("driver-engineer", "engineering", "Implementar drivers de dispositivo", &["pci","mmio","dma"]),
            spec("kernel-hacker", "engineering", "Otimizar o microkernel", &["scheduler","memory","ipc"]),
        ]});
        d.push(Division { name: String::from("design"), agents: vec![
            spec("ui-designer", "design", "Criar interfaces para o NeuralConsole", &["framebuffer","font","layout"]),
            spec("visual-identity", "design", "Identidade visual do Hermes", &["color","typography","branding"]),
        ]});
        d.push(Division { name: String::from("product"), agents: vec![
            spec("product-manager", "product", "Roadmap e prioridades", &["roadmap","sprint","backlog"]),
            spec("user-researcher", "product", "UX do Hermes", &["ux","feedback","metrics"]),
        ]});
        d.push(Division { name: String::from("qa"), agents: vec![
            spec("test-engineer", "qa", "Garantir zero panics", &["qemu","panic","stress"]),
            spec("security-auditor", "qa", "Auditar trust e permissoes", &["trust","capability","ed25519"]),
        ]});
        d.push(Division { name: String::from("support"), agents: vec![
            spec("help-desk", "support", "Ajudar usuario com comandos", &["help","docs","faq"]),
            spec("debug-assistant", "support", "Analisar panics", &["selfheal","panic","log"]),
        ]});
        d.push(Division { name: String::from("marketing"), agents: vec![
            spec("tech-writer", "marketing", "Documentacao e changelog", &["docs","md","changelog"]),
            spec("community-manager", "marketing", "Issues e contribuicoes", &["github","pr","review"]),
        ]});
        d.push(Division { name: String::from("infrastructure"), agents: vec![
            spec("devops", "infrastructure", "Builds e testes automatizados", &["qemu","ci","bootimage"]),
            spec("network-admin", "infrastructure", "Pilha de rede", &["smoltcp","dhcp","dns"]),
        ]});
        d.push(Division { name: String::from("data-science"), agents: vec![
            spec("ml-engineer", "data-science", "Treinar modelos BitNet", &["transformer","bitnet","training"]),
            spec("data-pipeline", "data-science", "Preparar datasets", &["dataset","pci","usb"]),
        ]});
        d.push(Division { name: String::from("creative"), agents: vec![
            spec("content-creator", "creative", "Conteudo do Hermes", &["llm","prompt","response"]),
            spec("game-designer", "creative", "Jogos no NeuralConsole", &["framebuffer","input","sprite"]),
        ]});
        d.push(Division { name: String::from("legal"), agents: vec![
            spec("compliance", "legal", "Licencas e permissoes", &["license","mit","trust"]),
        ]});
        d.push(Division { name: String::from("spatial"), agents: vec![
            spec("ar-engineer", "spatial", "Realidade aumentada", &["framebuffer","camera","compute"]),
        ]});
        d.push(Division { name: String::from("research"), agents: vec![
            spec("ai-researcher", "research", "Novas arquiteturas de IA", &["transformer","medusa","attention"]),
            spec("systems-researcher", "research", "Tecnicas de SO", &["scheduler","memory","ipc"]),
        ]});
        d
    }

    pub fn find(&self, name: &str) -> Option<&AgentSpec> {
        for div in &self.divisions { for a in &div.agents { if a.name == name { return Some(a); } } }
        None
    }

    pub fn for_task(&self, task: &str) -> Vec<&AgentSpec> {
        let lower = task.to_ascii_lowercase();
        let mut result = Vec::new();
        for div in &self.divisions {
            for a in &div.agents {
                if a.skills.iter().any(|s| lower.contains(s.as_str())) || a.mission.to_ascii_lowercase().contains(&lower) {
                    result.push(a);
                }
            }
        }
        result.truncate(5);
        result
    }

    pub fn llm_context(&self) -> String {
        let mut ctx = String::from("The Agency — agentes especializados:\n");
        for div in &self.divisions {
            ctx.push_str(&alloc::format!("\n[{}]\n", div.name));
            for a in &div.agents {
                ctx.push_str(&alloc::format!("  {}: {}\n", a.name, a.mission));
            }
        }
        ctx
    }
}

fn spec(name: &str, division: &str, mission: &str, skills: &[&str]) -> AgentSpec {
    AgentSpec {
        name: String::from(name), division: String::from(division),
        mission: String::from(mission),
        skills: skills.iter().map(|s| String::from(*s)).collect(),
        deliverable: String::from("Entregavel pendente"),
    }
}
