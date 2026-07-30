//! Host capability detection.
//!
//! V1 targets macOS, so this reads `sysctl` directly rather than taking on a
//! cross-platform dependency. Everything the rest of Kuro needs goes through
//! [`HardwareInfo`], so adding Windows or Linux later means adding one branch
//! in [`detect`] rather than touching call sites.

use std::process::Command;

use serde::Serialize;

pub mod fit;

pub use fit::{estimate_fit, FitEstimate, FitVerdict};

#[derive(Debug, Clone, Serialize)]
pub struct HardwareInfo {
    pub os: String,
    pub arch: String,
    pub chip: Option<String>,
    pub total_memory_bytes: u64,
    pub physical_cores: usize,
    pub logical_cores: usize,
    /// Whether the GPU can be used for offload. On Apple Silicon this also
    /// means GPU and CPU share one memory pool, which is why the fit estimate
    /// budgets against total system memory.
    pub gpu_available: bool,
    pub gpu_backend: &'static str,
    /// Settings Kuro will use when the user leaves engine options on "Auto".
    pub recommended: RecommendedEngineDefaults,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecommendedEngineDefaults {
    pub context_size: u32,
    /// Layers to offload to the GPU; 999 means "all of them".
    pub gpu_layers: i32,
    pub threads: u32,
}

pub fn detect() -> HardwareInfo {
    let total_memory_bytes = sysctl_number("hw.memsize").unwrap_or(8 * 1024 * 1024 * 1024);
    let physical_cores = sysctl_number("hw.physicalcpu").unwrap_or(4) as usize;
    let logical_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(physical_cores);

    let gpu_available = cfg!(target_os = "macos");

    HardwareInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        chip: sysctl_string("machdep.cpu.brand_string"),
        total_memory_bytes,
        physical_cores,
        logical_cores,
        gpu_available,
        gpu_backend: if gpu_available { "metal" } else { "cpu" },
        recommended: RecommendedEngineDefaults {
            context_size: 4096,
            // llama.cpp clamps this to the model's real layer count, so asking
            // for more than exists is the documented way to say "offload all".
            gpu_layers: if gpu_available { 999 } else { 0 },
            // Leave headroom rather than saturating every core, which keeps the
            // machine responsive while generating.
            threads: physical_cores.max(1) as u32,
        },
    }
}

fn sysctl_string(key: &str) -> Option<String> {
    let output = Command::new("/usr/sbin/sysctl").arg("-n").arg(key).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn sysctl_number(key: &str) -> Option<u64> {
    sysctl_string(key)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_plausible_hardware() {
        let info = detect();
        assert!(info.total_memory_bytes > 0, "memory must be detected");
        assert!(info.physical_cores >= 1);
        assert!(info.logical_cores >= info.physical_cores.min(info.logical_cores));
        assert!(info.recommended.threads >= 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn offloads_to_metal_on_macos() {
        let info = detect();
        assert!(info.gpu_available);
        assert_eq!(info.gpu_backend, "metal");
        assert_eq!(info.recommended.gpu_layers, 999);
    }
}
