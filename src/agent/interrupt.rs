//! Per-session interrupt and guidance-injection system.
//!
//! # Design
//!
//! Two-layer design:
//! 1. Each Agent instance owns an `Arc<AtomicBool>` for its own interrupt flag.
//! 2. A global registry holds `Weak` references to all active session flags.
//!    `request_interrupt()` sets every registered flag, so Ctrl-C (or a
//!    WebSocket "cancel" message) interrupts all sessions in the same process.
//! 3. `INTERRUPT_REQUESTED` (global AtomicBool) remains as a catch-all for
//!    code paths that don't have access to an Agent reference (streaming,
//!    call_node, etc.).  It is also set by `request_interrupt()`.
//!
//! In server mode each connection runs in a forked worker *process*, so the
//! globals are naturally isolated.  This registry is future-proofing for a
//! potential multi-session-in-one-process mode.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Weak};

static INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Registry of per-session interrupt flags.  Each Agent registers its flag
/// on creation (via `register_session_interrupt`) and the weak reference is
/// automatically cleaned up when the Agent is dropped.
static SESSION_INTERRUPTS: std::sync::LazyLock<Mutex<Vec<Weak<AtomicBool>>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// Register a per-session interrupt flag in the global registry.
///
/// Called by Agent constructors.  Dead weak references are cleaned up
/// on each registration.
pub(crate) fn register_session_interrupt(flag: &Arc<AtomicBool>) {
    if let Ok(mut guard) = SESSION_INTERRUPTS.lock() {
        // Clean up any dead weak references
        guard.retain(|w| w.upgrade().is_some());
        guard.push(Arc::downgrade(flag));
    }
}

/// Request interruption of all active sessions.
///
/// Sets the global `INTERRUPT_REQUESTED` flag and every registered per-session
/// flag.  Called by: Ctrl-C handler, WebSocket "cancel" message.
pub fn request_interrupt() {
    INTERRUPT_REQUESTED.store(true, Ordering::Relaxed);
    if let Ok(guard) = SESSION_INTERRUPTS.lock() {
        for weak in guard.iter() {
            if let Some(flag) = weak.upgrade() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Clear the interrupt flag globally and for all sessions.
pub fn clear_interrupt() {
    INTERRUPT_REQUESTED.store(false, Ordering::Relaxed);
    if let Ok(guard) = SESSION_INTERRUPTS.lock() {
        for weak in guard.iter() {
            if let Some(flag) = weak.upgrade() {
                flag.store(false, Ordering::Relaxed);
            }
        }
    }
}

/// Check whether the global interrupt flag is set.
///
/// Used by code paths that don't have access to an Agent reference
/// (streaming, call_node, etc.).
pub fn is_interrupted() -> bool {
    INTERRUPT_REQUESTED.load(Ordering::Relaxed)
}

// ── Guidance injection (Ctrl-\) ────────────────────────────────────────────

/// Set by the Ctrl-\ (SIGQUIT) handler; checked between LLM iterations in the
/// pipeline executor so the user can inject real-time guidance at any safe point.
static GUIDANCE_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_guidance() {
    GUIDANCE_REQUESTED.store(true, Ordering::Relaxed);
}

pub fn clear_guidance() {
    GUIDANCE_REQUESTED.store(false, Ordering::Relaxed);
}

pub fn is_guidance_requested() -> bool {
    GUIDANCE_REQUESTED.load(Ordering::Relaxed)
}
