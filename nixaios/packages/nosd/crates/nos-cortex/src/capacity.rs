//! Capacity Prober — /proc + sysfs DRM + nvidia-smi best-effort (sem NVML linkado).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

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
        if Path::new("/proc/driver/nvidia/gpus").exists() {
            c.vendor_hint = "nvidia".into();
        }
        // Best-effort: nvidia-smi (WSL/host) sem linkar NVML — honesty 0113.
        c.vram_free_mb = probe_nvidia_smi_free_mb().unwrap_or(0);
        if c.vram_free_mb > 0 && c.vendor_hint == "cpu" {
            c.vendor_hint = "nvidia".into();
        }
        c
    }

    /// Mensagem honesta para logs (VRAM=0 não implica "sem GPU").
    pub fn vram_posture(c: &NodeCapacity) -> &'static str {
        if c.vram_free_mb > 0 {
            "nvidia_smi"
        } else if c.vendor_hint == "nvidia"
            || Path::new("/proc/driver/nvidia/gpus").exists()
            || Path::new("/sys/module/nvidia").exists()
        {
            "nvml_absent"
        } else if !c.dri_render.is_empty() {
            "drm_present_vram_unknown"
        } else {
            "no_gpu_hint"
        }
    }
}

/// `nvidia-smi --query-gpu=memory.free --format=csv,noheader,nounits`
fn probe_nvidia_smi_free_mb() -> Option<u64> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // multi-GPU: soma free (hint; offload real exige build CUDA)
    let mut total = 0u64;
    let mut any = false;
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Ok(v) = t.parse::<u64>() {
            total = total.saturating_add(v);
            any = true;
        }
    }
    any.then_some(total)
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

    #[test]
    fn posture_vram_measured() {
        let c = NodeCapacity {
            vram_free_mb: 4000,
            vendor_hint: "nvidia".into(),
            ..Default::default()
        };
        assert_eq!(CapacityProber::vram_posture(&c), "nvidia_smi");
    }
}
