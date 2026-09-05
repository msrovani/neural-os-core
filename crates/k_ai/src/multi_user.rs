//! Multi-User + Multi-Persona — #187. Vários usuários com memória isolada, trust tiers.
//!
//! Canônico em k_ai (Ring cognitivo). Removido de k_nano na Fase 1 microkernel.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;

#[derive(Clone)]
pub struct UserProfile {
    pub name: String,
    pub trust_tier: u8,
    pub memory: Vec<String>,
    pub skills_allowed: Vec<String>,
}

pub struct UserManager {
    users: BTreeMap<String, UserProfile>,
    active: String,
}

impl UserManager {
    pub fn new() -> Self {
        let mut users = BTreeMap::new();
        users.insert(
            String::from("admin"),
            UserProfile {
                name: String::from("admin"),
                trust_tier: 5,
                memory: Vec::new(),
                skills_allowed: vec![String::from("*")],
            },
        );
        UserManager {
            users,
            active: String::from("admin"),
        }
    }
    pub fn add_user(&mut self, name: &str, tier: u8) {
        self.users.insert(
            String::from(name),
            UserProfile {
                name: String::from(name),
                trust_tier: tier,
                memory: Vec::new(),
                skills_allowed: Vec::new(),
            },
        );
    }
    pub fn switch(&mut self, name: &str) -> bool {
        if self.users.contains_key(name) {
            self.active = String::from(name);
            true
        } else {
            false
        }
    }
    pub fn active_user(&self) -> Option<&UserProfile> {
        self.users.get(&self.active)
    }
    pub fn can_execute(&self, skill: &str) -> bool {
        self.users.get(&self.active).map_or(false, |u| {
            u.trust_tier >= 3 || u.skills_allowed.iter().any(|s| s == "*" || s == skill)
        })
    }
    pub fn status(&self) -> String {
        alloc::format!("[USERS] {} users, active={}", self.users.len(), self.active)
    }
}
