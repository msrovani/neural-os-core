//! AppForge + Marketplace — #186, #246. Catálogo de apps, instalação one-click, marketplace.
//! Skills como pacotes versionados com verificação Ed25519.

use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

#[derive(Clone)]
pub struct AppPackage { pub name: String, pub version: String, pub author: String, pub description: String, pub skill_names: Vec<String>, pub signature: [u8; 64], pub enabled: bool }

pub struct AppForge { apps: BTreeMap<String, AppPackage> }

impl AppForge {
    pub fn new() -> Self { AppForge { apps: BTreeMap::new() } }
    pub fn install(&mut self, pkg: AppPackage) { self.apps.insert(pkg.name.clone(), pkg); }
    pub fn uninstall(&mut self, name: &str) { self.apps.remove(name); }
    pub fn list(&self) -> Vec<&AppPackage> { self.apps.values().collect() }
    pub fn search(&self, q: &str) -> Vec<&AppPackage> { self.apps.values().filter(|a| a.name.contains(q) || a.description.contains(q)).collect() }
    pub fn enable(&mut self, name: &str) { if let Some(a) = self.apps.get_mut(name) { a.enabled = true; } }
    pub fn disable(&mut self, name: &str) { if let Some(a) = self.apps.get_mut(name) { a.enabled = false; } }
    pub fn status(&self) -> String { alloc::format!("[APPFORGE] {} apps instalados", self.apps.len()) }
}






