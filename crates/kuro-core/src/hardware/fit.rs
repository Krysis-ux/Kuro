//! Rough "will this model run here?" estimate.
//!
//! This is deliberately a coarse heuristic, not a benchmark. It answers the one
//! question a model card needs to answer — is downloading this a good idea? —
//! and the UI labels it as approximate. Real throughput depends on the model
//! architecture, context length and what else is running.

use serde::Serialize;

use super::HardwareInfo;

/// Weights are not the whole cost: the KV cache, compute buffers and the
/// allocator's slack all sit on top. This multiplier covers that.
const OVERHEAD_MULTIPLIER: f64 = 1.15;

/// Roughly what the KV cache and runtime buffers cost at the default 4k
/// context. Larger contexts cost more, which is why the estimate is a floor.
const BASE_RUNTIME_OVERHEAD_BYTES: f64 = 1.0 * 1024.0 * 1024.0 * 1024.0;

/// Memory the operating system and other applications need to stay responsive.
const SYSTEM_RESERVE_BYTES: f64 = 3.0 * 1024.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitVerdict {
    /// Comfortable headroom; should run well.
    Great,
    /// Fits with room to spare for a normal context.
    Fits,
    /// Will load but leaves little headroom; expect slowdowns.
    Tight,
    /// Not enough memory on this machine.
    WontFit,
}

impl FitVerdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Great => "Great fit",
            Self::Fits => "Fits",
            Self::Tight => "Tight",
            Self::WontFit => "Won't fit",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FitEstimate {
    pub verdict: FitVerdict,
    pub label: &'static str,
    /// Approximate memory the model needs once loaded.
    pub estimated_required_bytes: u64,
    /// Memory available to models after reserving room for the system.
    pub usable_bytes: u64,
    pub note: String,
}

/// Estimate whether a model of `model_file_bytes` will run on `hardware`.
pub fn estimate_fit(model_file_bytes: u64, hardware: &HardwareInfo) -> FitEstimate {
    let required =
        (model_file_bytes as f64) * OVERHEAD_MULTIPLIER + BASE_RUNTIME_OVERHEAD_BYTES;
    let usable = (hardware.total_memory_bytes as f64 - SYSTEM_RESERVE_BYTES).max(0.0);

    let ratio = if usable > 0.0 {
        required / usable
    } else {
        f64::INFINITY
    };

    let verdict = if ratio <= 0.5 {
        FitVerdict::Great
    } else if ratio <= 0.75 {
        FitVerdict::Fits
    } else if ratio <= 1.0 {
        FitVerdict::Tight
    } else {
        FitVerdict::WontFit
    };

    let note = match verdict {
        FitVerdict::Great => "Comfortable on this machine.".to_string(),
        FitVerdict::Fits => "Should run well at the default context size.".to_string(),
        FitVerdict::Tight => {
            "Will load, but leaves little headroom. Consider a smaller quantization or a shorter context."
                .to_string()
        }
        FitVerdict::WontFit => format!(
            "Needs about {} but only about {} is available.",
            format_bytes(required as u64),
            format_bytes(usable as u64)
        ),
    };

    FitEstimate {
        verdict,
        label: verdict.label(),
        estimated_required_bytes: required as u64,
        usable_bytes: usable as u64,
        note,
    }
}

fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.1} GB", bytes as f64 / GB)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::RecommendedEngineDefaults;

    fn machine_with_gb(gb: u64) -> HardwareInfo {
        HardwareInfo {
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
            chip: None,
            total_memory_bytes: gb * 1024 * 1024 * 1024,
            physical_cores: 10,
            logical_cores: 10,
            gpu_available: true,
            gpu_backend: "metal",
            recommended: RecommendedEngineDefaults {
                context_size: 4096,
                gpu_layers: 999,
                threads: 10,
            },
        }
    }

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn small_model_on_a_large_machine_is_a_great_fit() {
        let estimate = estimate_fit(2 * GB, &machine_with_gb(24));
        assert_eq!(estimate.verdict, FitVerdict::Great);
    }

    #[test]
    fn oversized_model_is_rejected_with_an_explanation() {
        let estimate = estimate_fit(60 * GB, &machine_with_gb(16));
        assert_eq!(estimate.verdict, FitVerdict::WontFit);
        assert!(estimate.note.contains("available"));
    }

    #[test]
    fn verdict_degrades_monotonically_as_the_model_grows() {
        let machine = machine_with_gb(24);
        let mut previous = FitVerdict::Great;
        for gb in [1u64, 6, 12, 18, 30] {
            let verdict = estimate_fit(gb * GB, &machine).verdict;
            assert!(
                verdict as u8 >= previous as u8,
                "a bigger model must never look like a better fit"
            );
            previous = verdict;
        }
    }

    #[test]
    fn tiny_machine_cannot_run_a_mid_size_model() {
        let estimate = estimate_fit(5 * GB, &machine_with_gb(8));
        assert_eq!(estimate.verdict, FitVerdict::WontFit);
    }
}
