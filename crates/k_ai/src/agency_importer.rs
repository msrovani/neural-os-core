//! Agency Importer — importa agentes do repositório msitarzewski/agency-agents.
//! Fonte: https://github.com/msitarzewski/agency-agents (123k★, MIT)
//! Cada agente é um `AgentSpec` com nome, divisão, missão e skills.
//! Integrado ao `Agency::extend()` para adicionar divisões importadas.

use crate::agency::{AgentSpec, Division};
use alloc::vec;
use alloc::vec::Vec;

/// Importa todas as divisões do agency-agents para o Agency
pub fn import_divisions() -> Vec<Division> {
    vec![
        engineering_import(),
        sales_import(),
        marketing_import(),
        security_import(),
        testing_import(),
        project_management_import(),
        specialized_import(),
    ]
}

fn spec(name: &str, div: &str, mission: &str, skills: &[&str]) -> AgentSpec {
    AgentSpec {
        name: alloc::string::String::from(name),
        division: alloc::string::String::from(div),
        mission: alloc::string::String::from(mission),
        skills: skills.iter().map(|s| alloc::string::String::from(*s)).collect(),
        deliverable: alloc::string::String::from("auto"),
    }
}

fn engineering_import() -> Division {
    Division { name: alloc::string::String::from("engineering-imported"), agents: vec![
        spec("frontend-developer","engineering-imported","React/Vue/Angular, UI implementation, performance",
            &["react","vue","angular","css","typescript","ui","component"]),
        spec("backend-architect","engineering-imported","API design, database architecture, scalability",
            &["api","database","microservices","scalability","rest"]),
        spec("mobile-app-builder","engineering-imported","iOS/Android, React Native, Flutter",
            &["mobile","ios","android","react-native","flutter"]),
        spec("ai-engineer","engineering-imported","ML models, deployment, AI integration",
            &["ml","model","deployment","pipeline","training"]),
        spec("devops-automator","engineering-imported","CI/CD, infrastructure automation, cloud ops",
            &["ci","cd","automation","infrastructure","cloud"]),
        spec("network-engineer-imp","engineering-imported","Cisco IOS, Juniper, Palo Alto configuration",
            &["cisco","juniper","palo-alto","bgp","ospf","acl"]),
        spec("rapid-prototyper","engineering-imported","Fast POC development, MVPs",
            &["prototype","mvp","poc","fast-iteration","hackathon"]),
        spec("senior-developer","engineering-imported","Laravel/Livewire, advanced patterns",
            &["laravel","livewire","php","advanced-patterns"]),
        spec("embedded-firmware-engineer","engineering-imported","Bare-metal, RTOS, ESP32/STM32",
            &["embedded","firmware","rtos","esp32","stm32","bare-metal"]),
        spec("incident-response-commander","engineering-imported","Incident management, post-mortems",
            &["incident","postmortem","oncall","monitoring"]),
        spec("solidity-smart-contract-engineer","engineering-imported","EVM contracts, gas optimization",
            &["solidity","evm","defi","smart-contract","gas"]),
        spec("technical-writer","engineering-imported","Developer docs, API reference, tutorials",
            &["docs","api-reference","tutorial","technical-writing"]),
        spec("code-reviewer-imp","engineering-imported","Constructive code review, security, maintainability",
            &["code-review","pr","quality","maintainability"]),
        spec("database-optimizer","engineering-imported","Schema design, query optimization, indexing",
            &["database","postgresql","mysql","query","index","schema"]),
        spec("git-workflow-master","engineering-imported","Branching strategies, conventional commits",
            &["git","branching","conventional-commits","workflow"]),
        spec("software-architect","engineering-imported","System design, DDD, architectural patterns",
            &["architecture","ddd","system-design","patterns"]),
        spec("sre-engineer","engineering-imported","SLOs, error budgets, observability",
            &["slo","observability","reliability","chaos-engineering"]),
        spec("data-engineer","engineering-imported","Data pipelines, lakehouse architecture",
            &["data","pipeline","etl","lakehouse","warehouse"]),
        spec("prompt-engineer","engineering-imported","LLM prompt design & optimization",
            &["prompt","llm","optimization","few-shot","chain-of-thought"]),
        spec("multi-agent-systems-architect","engineering-imported","Multi-agent pipeline design & governance",
            &["multi-agent","pipeline","topology","governance","trust"]),
    ]}
}

fn sales_import() -> Division {
    Division { name: alloc::string::String::from("sales-imported"), agents: vec![
        spec("outbound-strategist","sales-imported","Signal-based prospecting, multi-channel sequences",
            &["outbound","prospecting","sequences","icp"]),
        spec("discovery-coach","sales-imported","SPIN, Gap Selling, Sandler — question design",
            &["discovery","spin","sandler","qualification","calls"]),
        spec("deal-strategist","sales-imported","MEDDPICC qualification, competitive positioning",
            &["meddpicc","deal","competitive","win-plan"]),
        spec("sales-engineer","sales-imported","Technical demos, POC scoping, battlecards",
            &["demo","poc","technical-sales","battlecard"]),
        spec("proposal-strategist","sales-imported","RFP response, win themes, narrative structure",
            &["rfp","proposal","win-theme","narrative"]),
        spec("pipeline-analyst","sales-imported","Forecasting, pipeline health, deal velocity",
            &["forecast","pipeline","velocity","revenue"]),
        spec("account-strategist","sales-imported","Land-and-expand, QBRs, stakeholder mapping",
            &["account","expansion","qbr","stakeholder"]),
        spec("sales-coach","sales-imported","Rep development, call coaching, pipeline review",
            &["coaching","rep-development","call-review","pipeline"]),
    ]}
}

fn marketing_import() -> Division {
    Division { name: alloc::string::String::from("marketing-imported"), agents: vec![
        spec("content-creator","marketing-imported","Multi-platform content, editorial calendars",
            &["content","editorial","copywriting","brand"]),
        spec("twitter-engager","marketing-imported","Real-time engagement, thought leadership",
            &["twitter","social","engagement","thought-leadership"]),
        spec("seo-specialist-imp","marketing-imported","Technical SEO, content strategy, link building",
            &["seo","organic","link-building","technical-seo"]),
        spec("growth-hacker-imp","marketing-imported","Rapid user acquisition, viral loops, experiments",
            &["growth","acquisition","viral","experiment","a-b-test"]),
        spec("app-store-optimizer","marketing-imported","ASO, conversion optimization, discoverability",
            &["aso","app-store","conversion","optimization"]),
        spec("email-marketing-strategist","marketing-imported","Lifecycle email & deliverability",
            &["email","lifecycle","deliverability","automation","campaign"]),
        spec("brand-guardian","marketing-imported","Brand identity, consistency, positioning",
            &["brand","identity","positioning","guidelines"]),
        spec("podcast-strategist","marketing-imported","Podcast content strategy, platform optimization",
            &["podcast","audio","content","distribution"]),
    ]}
}

fn security_import() -> Division {
    Division { name: alloc::string::String::from("security-imported"), agents: vec![
        spec("security-architect-imp","security-imported","Threat modeling, secure-by-design",
            &["threat-model","secure-design","trust-boundary","zero-trust"]),
        spec("appsec-engineer","security-imported","SDLC security, SAST/DAST, secure code review",
            &["appsec","sast","dast","code-review","sdlc"]),
        spec("penetration-tester-imp","security-imported","Authorized pentests, red team ops",
            &["pentest","red-team","exploitation","vulnerability"]),
        spec("cloud-security-architect","security-imported","Zero trust, cloud-native defense",
            &["cloud-security","zero-trust","iac","container"]),
        spec("incident-responder-imp","security-imported","DFIR, breach investigation, containment",
            &["dfir","forensics","incident-response","containment"]),
        spec("threat-intelligence-analyst","security-imported","Adversary tracking, campaign mapping",
            &["threat-intel","adversary","campaign","attack-pattern"]),
        spec("threat-detection-engineer","security-imported","SIEM rules, threat hunting",
            &["detection","siem","threat-hunting","rule"]),
        spec("compliance-auditor","security-imported","SOC 2, ISO 27001, HIPAA, PCI-DSS",
            &["compliance","soc2","iso27001","hipaa","pcidss"]),
        spec("blockchain-security-auditor","security-imported","Smart contract audits, exploit analysis",
            &["blockchain","smart-contract","audit","exploit"]),
    ]}
}

fn testing_import() -> Division {
    Division { name: alloc::string::String::from("testing-imported"), agents: vec![
        spec("evidence-collector","testing-imported","Screenshot-based QA, visual proof",
            &["qa","screenshot","visual","evidence"]),
        spec("reality-checker","testing-imported","Evidence-based certification, quality gates",
            &["certification","quality-gate","release","approval"]),
        spec("test-results-analyzer","testing-imported","Test evaluation, metrics analysis",
            &["test-analysis","metrics","coverage","insights"]),
        spec("performance-benchmarker","testing-imported","Performance testing, optimization",
            &["performance","benchmark","load-test","optimization"]),
        spec("api-tester","testing-imported","API validation, integration testing",
            &["api","integration","endpoint","validation"]),
        spec("accessibility-auditor","testing-imported","WCAG auditing, assistive technology",
            &["accessibility","wcag","screen-reader","inclusive"]),
        spec("workflow-optimizer","testing-imported","Process analysis, workflow improvement",
            &["workflow","process","efficiency","automation"]),
    ]}
}

fn project_management_import() -> Division {
    Division { name: alloc::string::String::from("pm-imported"), agents: vec![
        spec("studio-producer","pm-imported","High-level orchestration, portfolio management",
            &["orchestration","portfolio","strategic","resource"]),
        spec("project-shepherd","pm-imported","Cross-functional coordination, timeline",
            &["coordination","timeline","stakeholder","delivery"]),
        spec("studio-operations","pm-imported","Day-to-day efficiency, process optimization",
            &["operations","process","productivity","efficiency"]),
        spec("experiment-tracker","pm-imported","A/B tests, hypothesis validation",
            &["experiment","a-b-test","hypothesis","data-driven"]),
        spec("jira-workflow-steward","pm-imported","Git workflow, branch strategy, traceability",
            &["jira","git","workflow","branch","traceability"]),
        spec("meeting-notes-specialist","pm-imported","Structured meeting summaries",
            &["meeting","notes","decisions","action-items"]),
    ]}
}

fn specialized_import() -> Division {
    Division { name: alloc::string::String::from("specialized-imported"), agents: vec![
        spec("whimsy-injector","specialized-imported","Personality, delight, playful interactions",
            &["whimsy","delight","micro-interactions","personality","fun"]),
        spec("image-prompt-engineer","specialized-imported","AI image generation prompts",
            &["midjourney","dalle","stable-diffusion","prompt","photography"]),
        spec("inclusive-visuals-specialist","specialized-imported","Representation, bias mitigation",
            &["inclusive","representation","bias","authentic","cultural"]),
        spec("voice-ai-integration-engineer","specialized-imported","Speech-to-text, Whisper, ASR",
            &["voice","stt","whisper","asr","diarization"]),
        spec("it-service-manager","specialized-imported","ITIL 4 service management",
            &["itil","incident","problem","change","cmdb"]),
        spec("minimal-change-engineer","specialized-imported","Minimum-viable diffs, no scope creep",
            &["minimal","diff","scope","focused"]),
        spec("sprint-prioritizer","specialized-imported","Agile planning, feature prioritization",
            &["agile","sprint","prioritization","backlog","estimation"]),
        spec("trend-researcher","specialized-imported","Market intelligence, competitive analysis",
            &["research","trend","intelligence","competitive","market"]),
        spec("feedback-synthesizer","specialized-imported","User feedback analysis, insights extraction",
            &["feedback","analysis","insights","synthesis","user-research"]),
        spec("behavioral-nudge-engineer","specialized-imported","Behavioral psychology, nudge design",
            &["behavioral","nudge","psychology","engagement","motivation"]),
    ]}
}
