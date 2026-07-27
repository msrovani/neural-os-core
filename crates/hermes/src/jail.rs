//! JAIL — sandbox com Membrane + wasmi + Merkle audit (ADR-0076 F4).
//! AIOS na veia: zero operação sem verificação, toda operação auditada.
//!
//! Toda execução passa por:
//!   1. Membrane::check() → Allow/Deny/Escalate
//!   2. Se Allow: executa com fuel + heap limits
//!   3. Audit trail: cada operação tem entrada Merkle-verificável

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::membrane::{Membrane, Operation, Verdict, Capability};

/// Estado de uma execução na JAIL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JailStatus {
    Idle,
    Running { fuel_remaining: u64 },
    Blocked { reason: &'static str },
    Completed,
    Failed { error: &'static str },
}

/// Resultado de uma operação na JAIL.
#[derive(Debug, Clone)]
pub struct JailAuditEntry {
    pub operation_name: String,
    pub verdict: &'static str,  // "allow" | "deny" | "escalate"
    pub detail: String,
    pub tick: u64,
}

/// JAIL — sandbox que executa skills/apps com Membrane.
#[derive(Debug, Clone)]
pub struct Jail {
    pub name: String,
    pub membrane: Membrane,
    pub status: JailStatus,
    pub audit_log: Vec<JailAuditEntry>,
    ops_allowed: u64,
    ops_denied: u64,
}

impl Jail {
    /// Cria uma JAIL para app legacy — zero capacidades.
    pub fn for_legacy(app_name: &str) -> Self {
        Self {
            name: alloc::format!("jail_{}", app_name),
            membrane: Membrane::for_legacy(app_name),
            status: JailStatus::Idle,
            audit_log: Vec::new(),
            ops_allowed: 0,
            ops_denied: 0,
        }
    }
    
    /// Cria uma JAIL para skill WASM com capacidades específicas.
    pub fn for_wasm(name: &str, caps: Vec<Capability>) -> Self {
        Self {
            name: alloc::format!("jail_{}", name),
            membrane: Membrane::for_wasm(name, caps),
            status: JailStatus::Idle,
            audit_log: Vec::new(),
            ops_allowed: 0,
            ops_denied: 0,
        }
    }
    
    /// Verifica uma operação contra a membrana e audita.
    pub fn check(&mut self, op: &Operation) -> Verdict {
        let verdict = self.membrane.check(op);
        let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        
        let entry = JailAuditEntry {
            operation_name: String::from(op.name()),
            verdict: match verdict {
                Verdict::Allow => "allow",
                Verdict::Deny => "deny",
                Verdict::Escalate => "escalate",
            },
            detail: alloc::format!("{:?}", op),
            tick,
        };
        
        match verdict {
            Verdict::Allow => self.ops_allowed += 1,
            Verdict::Deny => self.ops_denied += 1,
            Verdict::Escalate => {}
        }
        
        // Append to audit trail
        let audit_msg = alloc::format!("JAIL[{}] {:?} → {:?}", self.name, op, verdict);
        crate::globals::AUDIT_TRAIL.lock().push(tick, "jail", &audit_msg, &[]);
        
        self.audit_log.push(entry);
        verdict
    }
    
    /// Verifica acesso a arquivo.
    pub fn check_file_read(&mut self, path: &str) -> Verdict {
        self.check(&Operation::FileRead(String::from(path)))
    }
    
    /// Verifica escrita em arquivo.
    pub fn check_file_write(&mut self, path: &str) -> Verdict {
        self.check(&Operation::FileWrite(String::from(path)))
    }
    
    /// Verifica conexão de rede.
    pub fn check_net_connect(&mut self, host: &str, port: u16) -> Verdict {
        self.check(&Operation::NetConnect(String::from(host), port))
    }
    
    /// Verifica uso de capacidade.
    pub fn check_capability(&mut self, cap: Capability) -> Verdict {
        self.check(&Operation::CapabilityUse(cap))
    }
    
    /// Report estatístico da JAIL.
    pub fn report(&self) -> String {
        alloc::format!(
            "JAIL[{}] status={:?} allowed={} denied={} audit={}",
            self.name, self.status, self.ops_allowed, self.ops_denied, self.audit_log.len()
        )
    }
}

/// Self-test da JAIL.
pub fn demo() -> bool {
    // Teste 1: JAIL legacy — paths permitidos/negados
    let mut jail = Jail::for_legacy("test_app");
    assert!(jail.check_file_read("/jail/test_app/doc.txt") == Verdict::Allow);
    assert!(jail.check_file_read("/etc/passwd") == Verdict::Deny);
    assert!(jail.check_net_connect("google.com", 80) == Verdict::Deny);
    assert!(jail.check_capability(Capability::RawMmio) == Verdict::Deny);
    assert!(jail.check_capability(Capability::GpuCompute) == Verdict::Escalate);
    assert!(jail.ops_allowed == 1);
    assert!(jail.ops_denied >= 2);
    assert!(!jail.audit_log.is_empty());
    
    // Teste 2: JAIL WASM com capacidades
    let caps = vec![Capability::VfsRead, Capability::VfsWrite, Capability::NetTcp];
    let mut jail2 = Jail::for_wasm("office_skill", caps);
    assert!(jail2.check_file_read("/tmp/test.txt") == Verdict::Allow);
    assert!(jail2.check_file_read("/etc/passwd") == Verdict::Deny);
    assert!(jail2.check_capability(Capability::NetTcp) == Verdict::Allow);
    assert!(jail2.check_capability(Capability::AudioPlay) == Verdict::Escalate);
    
    // Teste 3: Report
    let report = jail2.report();
    assert!(report.contains("jail_office_skill"));
    assert!(report.contains("allowed=2")); // file_read + net_tcp
    
    true
}
