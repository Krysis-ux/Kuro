//! Inference engine supervision.
//!
//! Kuro runs models through `kuro-engine`, one process per loaded model,
//! managed entirely by [`manager::EngineManager`].

pub mod bootstrap;
pub mod manager;
pub mod port;
pub mod process;

pub use bootstrap::{ensure_engine, DEFAULT_ENGINE_TAG};
pub use manager::{EngineManager, LoadedEngine};
pub use port::allocate_port;
pub use process::EngineLaunchSpec;
