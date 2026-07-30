//! Internal port allocation for engine processes.
//!
//! Every loaded model gets its own `llama-server` on a loopback port that users
//! never see; they only ever talk to Kuro's own port. Ports are taken from a
//! private range and probed before use so Kuro does not collide with whatever
//! else is running on the machine.

use std::net::TcpListener;

use crate::{KuroError, Result};

/// Private range for engine processes. Chosen to sit above the common
/// development ports and below the ephemeral range macOS hands out.
pub const PORT_RANGE_START: u16 = 39200;
pub const PORT_RANGE_END: u16 = 39299;

/// Find a free loopback port in Kuro's private range.
///
/// `in_use` holds ports Kuro has already handed out but whose process may not
/// have bound yet, which a bind probe alone would not catch.
pub fn allocate_port(in_use: &[u16]) -> Result<u16> {
    for candidate in PORT_RANGE_START..=PORT_RANGE_END {
        if in_use.contains(&candidate) {
            continue;
        }
        if is_available(candidate) {
            return Ok(candidate);
        }
    }

    Err(KuroError::engine(format!(
        "no free port between {PORT_RANGE_START} and {PORT_RANGE_END}; \
         unload a model or restart Kuro"
    )))
}

/// Whether a port can currently be bound on loopback.
///
/// The listener is dropped immediately, so this is advisory: the engine process
/// binds a moment later. The `in_use` list in [`allocate_port`] covers the gap.
fn is_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_within_the_private_range() {
        let port = allocate_port(&[]).expect("a port should be free");
        assert!((PORT_RANGE_START..=PORT_RANGE_END).contains(&port));
    }

    #[test]
    fn skips_ports_already_handed_out() {
        let first = allocate_port(&[]).expect("first");
        let second = allocate_port(&[first]).expect("second");
        assert_ne!(first, second);
    }

    #[test]
    fn reports_exhaustion_rather_than_looping_forever() {
        let everything: Vec<u16> = (PORT_RANGE_START..=PORT_RANGE_END).collect();
        let error = allocate_port(&everything).unwrap_err();
        assert!(matches!(error, KuroError::Engine(_)));
    }

    #[test]
    fn does_not_hand_out_a_port_that_is_actually_bound() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let taken = listener.local_addr().expect("addr").port();

        // Only meaningful if the OS happened to pick a port in our range.
        if (PORT_RANGE_START..=PORT_RANGE_END).contains(&taken) {
            let port = allocate_port(&[]).expect("allocate");
            assert_ne!(port, taken);
        }
    }
}
