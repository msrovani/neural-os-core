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
        Agency { divisions: Self::build() }
    }

    fn build() -> Vec<Division> {
        let mut d = Vec::new();
        d.push(d_engineering());
        d.push(d_design());
        d.push(d_product());
        d.push(d_qa());
        d.push(d_support());
        d.push(d_marketing());
        d.push(d_infrastructure());
        d.push(d_data_science());
        d.push(d_creative());
        d.push(d_legal());
        d.push(d_spatial());
        d.push(d_research());
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

    pub fn count(&self) -> usize {
        self.divisions.iter().map(|d| d.agents.len()).sum()
    }

    pub fn llm_context(&self) -> String {
        let mut ctx = alloc::format!("The Agency — {} agentes, {} divisoes:\n", self.count(), self.divisions.len());
        for div in &self.divisions {
            ctx.push_str(&alloc::format!("\n[{}] ({} agents)\n", div.name, div.agents.len()));
            for a in &div.agents {
                ctx.push_str(&alloc::format!("  {}: {}\n", a.name, a.mission));
            }
        }
        ctx
    }
}

fn spec(name: &str, div: &str, mission: &str, skills: &[&str]) -> AgentSpec {
    AgentSpec { name: String::from(name), division: String::from(div), mission: String::from(mission),
        skills: skills.iter().map(|s| String::from(*s)).collect(), deliverable: String::from("auto") }
}

fn d_engineering() -> Division {
    Division { name: String::from("engineering"), agents: vec![
        spec("system-architect","engineering","Projetar arquitetura do sistema",&["memory","pci","smp","apic"]),
        spec("kernel-engineer","engineering","Desenvolver o microkernel",&["scheduler","ipc","syscall","memory"]),
        spec("driver-engineer","engineering","Implementar drivers PCI/ACPI",&["pci","mmio","dma","acpi"]),
        spec("network-engineer","engineering","Pilha TCP/IP e drivers de rede",&["smoltcp","rtl8139","dhcp","dns"]),
        spec("storage-engineer","engineering","Sistemas de arquivos e blocos",&["fat12","ata","nvme","ahci"]),
        spec("graphics-engineer","engineering","Framebuffer e compositor",&["framebuffer","virtio-gpu","bgra","bpp"]),
        spec("usb-engineer","engineering","Driver xHCI e dispositivos USB",&["xhci","hid","msc","hub"]),
        spec("smp-engineer","engineering","Inicializacao e balanceamento SMP",&["smp","apic","ipi","percpu"]),
        spec("security-engineer","engineering","TrustCache e pipeline de seguranca",&["trust","ed25519","capability","audit"]),
        spec("firmware-engineer","engineering","Bootloader e ACPI",&["bootloader","acpi","madt","xsdt"]),
        spec("compiler-engineer","engineering","Otimizacoes para o kernel Rust",&["rust","llvm","inline-asm","no-std"]),
        spec("toolchain-engineer","engineering","Ferramentas de build e debug",&["cargo","qemu","gdb","bootimage"]),
        spec("memory-engineer","engineering","Alocadores e hierarquia de memoria",&["heap","slab","frame","mhi"]),
        spec("interrupt-engineer","engineering","IDT, PIC, APIC e handlers",&["idt","pic","apic","eoi","ipi"]),
        spec("pci-engineer","engineering","Barramento PCI e dispositivos",&["pci","cfg","capability","bridge"]),
        spec("acpi-engineer","engineering","Tabelas ACPI e gerenciamento energia",&["acpi","madt","fadt","dsdt"]),
        spec("power-engineer","engineering","Estados de energia e sleep",&["power","acpi","cstate","pstate"]),
        spec("virtio-engineer","engineering","Drivers VirtIO (net,gpu,input)",&["virtio","pci","mmio","queue"]),
        spec("cpu-engineer","engineering","Detecção e configuração de CPU",&["cpuid","msr","features","topology"]),
        spec("atomic-engineer","engineering","Operacoes atomicas e locks",&["atomic","lock","fence","ticketlock"]),
        spec("time-engineer","engineering","Timers e clocks do sistema",&["lapic","pit","hpet","tick"]),
        spec("console-engineer","engineering","Terminal e entrada/saida",&["vga","serial","keyboard","framebuffer"]),
        spec("elf-loader","engineering","Carregador de ELF e linkeditor",&["elf","relocation","pie","linker"]),
        spec("panic-engineer","engineering","Tratamento de panics e excecoes",&["panic","handler","stacktrace","hlt"]),
        spec("test-engineer-eng","engineering","Testes unitarios do kernel",&["test","assert","qemu","kunit"]),
        spec("sanitizer-engineer","engineering","Sanitizacao de memoria e bounds",&["asan","ub","safe","checked"]),
    ]}
}

fn d_design() -> Division {
    Division { name: String::from("design"), agents: vec![
        spec("ui-designer","design","Layout do NeuralConsole e HUD",&["framebuffer","font","layout","console"]),
        spec("visual-identity","design","Identidade visual do Hermes",&["color","typography","branding","logo"]),
        spec("ux-engineer","design","Experiencia do usuario no terminal",&["prompt","shell","keyboard","scroll"]),
        spec("theme-designer","design","Temas de cor e estilo",&["theme","palette","contrast","a11y"]),
        spec("icon-designer","design","Icones e simbolos do sistema",&["icon","bitmap","pixel","glyph"]),
        spec("motion-designer","design","Animacoes e transicoes",&["framebuffer","animation","easing","fps"]),
        spec("font-designer","design","Tipografia bitmap para o console",&["font","bitmap","glyph","kerning"]),
        spec("information-architect","design","Estrutura de menus e comandos",&["shell","help","man","hierarchy"]),
        spec("interaction-designer","design","Padroes de interacao teclado/mouse",&["keyboard","mouse","shortcut","focus"]),
        spec("design-system","design","Sistema de design unificado",&["components","tokens","specs","docs"]),
        spec("layout-engineer","design","Layout responsivo do console",&["grid","flex","margin","padding"]),
        spec("color-theorist","design","Teoria das cores para HUD",&["contrast","brightness","hue","accessibility"]),
        spec("animation-designer","design","Animacoes de transicao",&["easing","keyframe","tween","transition"]),
    ]}
}

fn d_product() -> Division {
    Division { name: String::from("product"), agents: vec![
        spec("product-manager","product","Roadmap e prioridades",&["roadmap","sprint","backlog","okr"]),
        spec("user-researcher","product","Pesquisa com usuarios do Hermes",&["ux","feedback","metrics","survey"]),
        spec("product-analyst","product","Analise de metricas de uso",&["metrics","usage","events","kpi"]),
        spec("technical-product-manager","product","Requisitos tecnicos do kernel",&["kernel","api","feature","spec"]),
        spec("growth-hacker","product","Estrategias de adocao do SO",&["adoption","onboarding","virality","retention"]),
        spec("feature-request-manager","product","Gerenciamento de pedidos de features",&["feature","request","vote","priority"]),
        spec("sprint-planner","product","Planejamento de sprints",&["sprint","agile","backlog","estimation"]),
        spec("competitor-analyst","product","Analise de concorrentes",&["competitor","landscape","benchmark","research"]),
        spec("beta-tester-coordinator","product","Coordenacao de testes beta",&["beta","testing","feedback","release"]),
    ]}
}

fn d_qa() -> Division {
    Division { name: String::from("qa"), agents: vec![
        spec("test-engineer","qa","Testes de regressao em QEMU",&["qemu","test","panic","stress"]),
        spec("security-auditor","qa","Auditoria de trust e permissoes",&["trust","capability","ed25519","audit"]),
        spec("integration-tester","qa","Testes de integracao entre agentes",&["agents","eventbus","ipc","registry"]),
        spec("hardware-tester","qa","Testes em hardware real",&["qemu","pci","acpi","smp"]),
        spec("fuzz-tester","qa","Testes de fuzzing no kernel",&["fuzz","panic","memory","safety"]),
        spec("performance-benchmark","qa","Benchmarks de desempenho",&["benchmark","latency","throughput","tick"]),
        spec("code-reviewer","qa","Revisao de codigo e estilo",&["review","rust","clippy","style"]),
        spec("documentation-tester","qa","Testes de documentacao",&["docs","help","man","examples"]),
        spec("regression-bot","qa","Testes automaticos pos-commit",&["ci","qemu","panic","log"]),
        spec("penetration-tester","qa","Testes de penetracao",&["exploit","trust","capability","sandbox"]),
    ]}
}

fn d_support() -> Division {
    Division { name: String::from("support"), agents: vec![
        spec("help-desk","support","Ajuda com comandos do Hermes",&["help","docs","faq","shell"]),
        spec("debug-assistant","support","Analisar panics e erros",&["selfheal","panic","log","crash"]),
        spec("troubleshooter","support","Diagnostico de hardware",&["pci","acpi","apic","smp"]),
        spec("onboarding-guide","support","Guia de primeiro uso",&["tutorial","welcome","help","shell"]),
        spec("faq-bot","support","Perguntas frequentes",&["faq","help","docs","search"]),
        spec("error-translator","support","Traduzir panics para mensagens amigaveis",&["panic","error","message","ux"]),
        spec("remote-support","support","Suporte remoto via serial",&["serial","remote","debug","ssh"]),
        spec("self-heal-coach","support","Orientacao sobre auto-recuperacao",&["selfheal","recovery","checkpoint","restore"]),
        spec("compatibility-helper","support","Ajuda com compatibilidade de HW",&["pci","acpi","hardware","compat"]),
        spec("upgrade-assistant","support","Assistente de upgrades do SO",&["upgrade","migrate","version","update"]),
        spec("config-advisor","support","Ajuda com configuracao do Hermes",&["config","settings","preferences","tuning"]),
        spec("performance-advisor","support","Diagnostico de performance",&["performance","slow","memory","cpu"]),
    ]}
}

fn d_marketing() -> Division {
    Division { name: String::from("marketing"), agents: vec![
        spec("tech-writer","marketing","Documentacao tecnica e changelog",&["docs","md","changelog","adr"]),
        spec("community-manager","marketing","Issues, PRs e comunidade",&["github","pr","review","issues"]),
        spec("documentation-engineer","marketing","Documentacao do kernel",&["docs","rustdoc","adr","spec"]),
        spec("developer-relations","marketing","Relacoes com devs e contribuidores",&["github","discord","community","evangelism"]),
        spec("social-media-manager","marketing","Redes sociais do projeto",&["twitter","linkedin","youtube","blog"]),
        spec("seo-specialist","marketing","Otimizacao para buscadores",&["seo","docs","readme","keywords"]),
        spec("video-producer","marketing","Producao de videos do Hermes",&["video","demo","tutorial","youtube"]),
        spec("newsletter-writer","marketing","Newsletter do projeto",&["newsletter","updates","changelog","community"]),
    ]}
}

fn d_infrastructure() -> Division {
    Division { name: String::from("infrastructure"), agents: vec![
        spec("devops-engineer","infrastructure","CI/CD e automacao de builds",&["cargo","ci","qemu","bootimage"]),
        spec("network-admin","infrastructure","Configuracao de rede",&["smoltcp","dhcp","dns","ip"]),
        spec("build-engineer","infrastructure","Otimizacao de build",&["cargo","rustc","lld","linker"]),
        spec("release-manager","infrastructure","Gerenciamento de releases",&["version","tag","changelog","semver"]),
        spec("monitoring-agent","infrastructure","Monitoramento de sistema",&["tick","memory","cpu","uptime"]),
        spec("backup-agent","infrastructure","Backup e restauracao",&["backup","restore","checkpoint","snapshot"]),
        spec("container-orchestrator","infrastructure","Orquestracao de agentes",&["agent","scheduler","lifecycle","pool"]),
        spec("log-manager","infrastructure","Gerenciamento de logs",&["log","rotate","compress","retention"]),
        spec("update-agent","infrastructure","Atualizacoes do kernel",&["update","hotswap","version","checksum"]),
    ]}
}

fn d_data_science() -> Division {
    Division { name: String::from("data-science"), agents: vec![
        spec("ml-engineer","data-science","Treino de modelos BitNet",&["transformer","bitnet","training","pytorch"]),
        spec("data-pipeline","data-science","Preparacao de datasets",&["dataset","pci","usb","jsonl"]),
        spec("model-optimizer","data-science","Otimizacao de inferencia",&["quantization","ternary","medusa","packed"]),
        spec("training-pipeline","data-science","Pipeline de treino GPU/Colab",&["cuda","colab","t4","gpu"]),
        spec("dataset-curator","data-science","Curadoria de dados de hardware",&["pci","usb","acpi","smbios"]),
        spec("evaluator","data-science","Avaliacao de qualidade do modelo",&["loss","accuracy","benchmark","eval"]),
        spec("feature-engineer","data-science","Engenharia de features",&["features","embedding","tokenizer","vocab"]),
        spec("model-server","data-science","Servidor de inferencia",&["inference","serving","batch","latency"]),
        spec("ab-testing","data-science","Testes A/B de modelos",&["ab-test","experiment","variant","metric"]),
    ]}
}

fn d_creative() -> Division {
    Division { name: String::from("creative"), agents: vec![
        spec("content-creator","creative","Conteudo do Hermes LLM",&["llm","prompt","response","chat"]),
        spec("game-designer","creative","Jogos para o NeuralConsole",&["framebuffer","input","sprite","collision"]),
        spec("writer","creative","Textos e dialogos do sistema",&["copy","help","messages","tone"]),
        spec("music-designer","creative","Sound design para o SO",&["audio","beep","pcspeaker","midi"]),
        spec("storyteller","creative","Narrativa e lore do Hermes",&["story","lore","personality","voice"]),
        spec("meme-designer","creative","Memes e easter eggs",&["easteregg","humor","joke","fun"]),
        spec("ascii-artist","creative","Arte ASCII para o terminal",&["ascii","art","terminal","banner"]),
    ]}
}

fn d_legal() -> Division {
    Division { name: String::from("legal"), agents: vec![
        spec("compliance-officer","legal","Verificacao de licencas",&["license","mit","apache","gpl"]),
        spec("license-auditor","legal","Auditoria de dependencias",&["crates","license","origin","attribution"]),
        spec("data-privacy","legal","Privacidade de dados do usuario",&["privacy","data","gdpr","logs"]),
        spec("policy-advisor","legal","Politicas de uso do Hermes",&["policy","terms","trust","ethics"]),
        spec("contract-reviewer","legal","Revisao de contratos e EULA",&["contract","eula","terms","liability"]),
        spec("trademark-watch","legal","Vigilancia de marcas e patentes",&["trademark","patent","ip","brand"]),
        spec("regulatory-analyst","legal","Analise regulatoria por pais",&["regulation","law","compliance","jurisdiction"]),
        spec("disclaimer-generator","legal","Geracao de avisos legais",&["disclaimer","notice","liability","tos"]),
        spec("privacy-engineer","legal","Engenharia de privacidade de dados",&["credit","data","pii","anon"]),
    ]}
}

fn d_spatial() -> Division {
    Division { name: String::from("spatial"), agents: vec![
        spec("ar-engineer","spatial","Realidade aumentada no OS",&["framebuffer","camera","compute","overlay"]),
        spec("spatial-computing","spatial","Computacao espacial",&["framebuffer","depth","tracking","render"]),
        spec("computer-vision","spatial","Visao computacional basica",&["camera","frame","detect","track"]),
        spec("3d-renderer","spatial","Renderizacao 3D basica",&["framebuffer","triangle","z-buffer","mesh"]),
        spec("lidar-engineer","spatial","Processamento LiDAR",&["lidar","pointcloud","depth","mapping"]),
        spec("gesture-engineer","spatial","Reconhecimento de gestos",&["gesture","hand","tracking","input"]),
        spec("eye-tracking","spatial","Rastreamento ocular",&["eye","gaze","focus","attention"]),
        spec("hologram-engineer","spatial","Display holografico",&["hologram","lightfield","projection","3d"]),
        spec("spatial-audio","spatial","Audio espacial 3D",&["audio","spatial","3d","binaural"]),
    ]}
}

fn d_research() -> Division {
    Division { name: String::from("research"), agents: vec![
        spec("ai-researcher","research","Novas arquiteturas neurais",&["transformer","medusa","attention","bitnet"]),
        spec("systems-researcher","research","Pesquisa em SO",&["scheduler","memory","ipc","fs"]),
        spec("compiler-researcher","research","Otimizacoes em tempo de compilacao",&["rust","llvm","optimization","codegen"]),
        spec("security-researcher","research","Pesquisa em seguranca de kernel",&["exploit","mitigation","trust","isolation"]),
        spec("hardware-researcher","research","Suporte a novo hardware",&["pci","acpi","virtio","chipset"]),
        spec("quantum-researcher","research","Computacao quantica (futuro)",&["quantum","qubit","simulation","crypto"]),
        spec("programming-language","research","Linguagens para bare-metal",&["rust","no-std","alloc","panic"]),
        spec("distributed-systems","research","Sistemas distribuidos",&["cluster","network","sync","consensus"]),
        spec("formal-verification","research","Verificacao formal do kernel",&["model","checking","theorem","prover"]),
        spec("nlp-researcher","research","Processamento de linguagem natural",&["nlp","tokenizer","transformer","llm"]),
        spec("computer-vision-research","research","Pesquisa em visao computacional",&["vision","cnn","detection","tracking"]),
        spec("robotics-researcher","research","Integracao com robotica",&["robot","control","sensor","actuator"]),
        spec("hpc-researcher","research","Computacao de alto desempenho",&["hpc","parallel","simd","avx"]),
        spec("crypto-researcher","research","Criptografia e segurança",&["crypto","ed25519","aes","hash"]),
        spec("ai-safety-researcher","research","Segurança de IA",&["alignment","safety","ethics","control"]),
        spec("os-researcher","research","Pesquisa em sistemas operacionais",&["kernel","microkernel","unikernel","hypervisor"]),
        spec("network-researcher","research","Pesquisa em redes",&["protocol","tcp","udp","dns"]),
        spec("database-researcher","research","Sistemas de banco de dados",&["storage","index","query","kv"]),
        spec("programming-language-theory","research","Teoria de linguagens",&["pl","type","system","semantics"]),
        spec("agent-researcher","research","Sistemas multi-agente",&["agent","multi-agent","coordination","swarm"]),
        spec("swarm-researcher","research","Inteligencia de enxame",&["swarm","emergent","collective","stigmergy"]),
        spec("cybernetics-researcher","research","Cibernetica e sistemas",&["cybernetics","feedback","control","homeostasis"]),
        spec("fractal-researcher","research","Geometria fractal computacional",&["fractal","recursion","chaos","emergence"]),
        spec("complexity-researcher","research","Teoria da complexidade",&["complexity","emergence","phase","transition"]),
        spec("systems-theory","research","Teoria geral de sistemas",&["system","boundary","environment","wholeness"]),
    ]}
}
