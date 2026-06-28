use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub skills: Vec<String>,
    pub security_risk: u8,
    pub source_url: String,
}

pub struct PluginHub {
    pub installed: BTreeMap<String, PluginManifest>,
    pub remote_index: Vec<PluginManifest>,
}

impl PluginHub {
    pub fn new() -> Self {
        PluginHub { installed: BTreeMap::new(), remote_index: Vec::new() }
    }

    pub fn install(&mut self, name: &str, manifest: PluginManifest) -> bool {
        if self.installed.contains_key(name) { return false; }
        if manifest.security_risk > 7 { return false; }
        self.installed.insert(String::from(name), manifest);
        true
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.installed.remove(name).is_some()
    }

    pub fn scan_risk(&self, manifest: &PluginManifest) -> u8 {
        let mut risk: u8 = 0;
        for s in &manifest.skills {
            if s.contains("exec") || s.contains("shell") { risk += 3; }
            if s.contains("net") || s.contains("http") { risk += 2; }
            if s.contains("fs") || s.contains("file") { risk += 2; }
        }
        if manifest.source_url.contains("unknown") { risk += 2; }
        risk.min(10)
    }

    pub fn discover(&mut self, url: &str, skills: &[&str]) -> PluginManifest {
        let name = url.split('/').last().unwrap_or("unknown");
        let manifest = PluginManifest {
            name: String::from(name), version: String::from("1.0"),
            description: String::from(url),
            skills: skills.iter().map(|s| String::from(*s)).collect(),
            security_risk: 5,
            source_url: String::from(url),
        };
        let risk = self.scan_risk(&manifest);
        PluginManifest { security_risk: risk, ..manifest }
    }

    pub fn list(&self) -> Vec<&PluginManifest> {
        self.installed.values().collect()
    }
}
