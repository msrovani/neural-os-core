//! Hub Discovery + Multi-Instance Board + Observability — #241, #243.

use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

pub struct Observability { pub logs: Vec<String>, pub metrics: BTreeMap<String, f32> }
impl Observability {
    pub fn new() -> Self { Observability { logs: Vec::new(), metrics: BTreeMap::new() } }
    pub fn log(&mut self, msg: &str) { self.logs.push(String::from(msg)); while self.logs.len() > 1000 { self.logs.remove(0); } }
    pub fn gauge(&mut self, name: &str, val: f32) { self.metrics.insert(String::from(name), val); }
    pub fn status(&self) -> String { alloc::format!("[OBSERV] {} logs, {} metrics", self.logs.len(), self.metrics.len()) }
}

pub struct HubDiscovery { instances: Vec<(String, u64)> }
impl HubDiscovery {
    pub fn new() -> Self { HubDiscovery { instances: Vec::new() } }
    pub fn announce(&mut self, id: &str) { self.instances.push((String::from(id), 0)); }
    pub fn status(&self) -> String { alloc::format!("[HUB] {} instancias", self.instances.len()) }
}
