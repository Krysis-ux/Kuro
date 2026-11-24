//! Inference engine supervision: one `kuro-engine` process per loaded model,
//! managed by [`manager::EngineManager`].

pub mod bootstrap;
pub mod manager;
pub mod port;
pub mod process;

pub use bootstrap::{ensure_engine, DEFAULT_ENGINE_TAG};
pub use manager::{EngineManager, LoadedEngine};
pub use port::allocate_port;
pub use process::EngineLaunchSpec;
