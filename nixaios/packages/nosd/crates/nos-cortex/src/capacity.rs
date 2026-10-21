//! Capacity Prober — /proc + sysfs DRM (honest: NVML/ROCm opcional).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeCapacity {
    pub ram_total_mb: u64,
    pub ram_avail_mb: u64,
    pub vram_free_mb: u64,
    pub dri_cards: Vec<String>,
    pub dri_render: Vec<String>,
    pub vendor_hint: String,
}

pub struct CapacityProber;

impl CapacityProber {
    pub fn probe() -> NodeCapacity {
        let mut c = NodeCapacity::default();
        c.ram_total_mb = read_meminfo_kb("MemTotal").unwrap_or(0) / 1024;
        c.ram_avail_mb = read_meminfo_kb("MemAvailable")
            .or_else(|| read_meminfo_kb("MemFree"))
            .unwrap_or(0)
            / 1024;
        c.dri_cards = list_dri("card");
        c.dri_render = list_dri("renderD");
        c.vendor_hint = detect_vendor_hint();
        // Sem NVML linkado no scaffold: VRAM = 0 → offload planner usa CPU.
        c.vram_free_mb = 0;
        if Path::new("/proc/driver/nvidia/gpus").exists() {
            c.vendor_hint = "nvidia".into();
        }
        c
    }
}

fn read_meminfo_kb(key: &str) -> Option<u64> {
    let s = fs::read_to_string("/proc/meminfo").ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim().trim_start_matches(':').trim();
            let num: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(num);
        }
    }
    None
}

fn list_dri(prefix: &str) -> Vec<String> {
    let Ok(rd) = fs::read_dir("/dev/dri") else {
        return Vec::new();
    };
    let mut v: Vec<_> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix))
        .collect();
    v.sort();
    v
}

fn detect_vendor_hint() -> String {
    if Path::new("/sys/module/amdgpu").exists() {
        return "amd".into();
    }
    if Path::new("/sys/module/i915").exists() || Path::new("/sys/module/xe").exists() {
        return "intel".into();
    }
    if Path::new("/sys/module/nvidia").exists() {
        return "nvidia".into();
    }
    "cpu".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_does_not_panic() {
        let _ = CapacityProber::probe();
    }
}
