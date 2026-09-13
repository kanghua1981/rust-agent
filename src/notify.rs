//! Notifications pushed into the agent from outside the tool loop.
//!
//! Producers (currently the MCP client) call [`push`] from any task; the agent
//! drains the channel at safe points (`Agent::drain_notifications`) and forwards
//! each event to `AgentOutput::on_notification`, so nothing interrupts LLM
//! streaming mid-token.

use once_cell::sync::Lazy;
use tokio::sync::broadcast;

use crate::output::NotifyLevel;

/// Capacity of the broadcast channel. Old events are dropped when the buffer is
/// full: the agent is busy, and a few missed notifications are acceptable.
const CHANNEL_CAP: usize = 64;

/// A notification pushed into the agent's stream.
#[derive(Debug, Clone)]
pub struct Notification {
    pub source: String,
    pub level: NotifyLevel,
    pub message: String,
}

/// Any producer can push here; the agent subscribes via [`subscribe`].
static NOTIFICATION_TX: Lazy<broadcast::Sender<Notification>> = Lazy::new(|| {
    let (tx, _) = broadcast::channel(CHANNEL_CAP);
    tx
});

/// Subscribe to the notification stream.
pub fn subscribe() -> broadcast::Receiver<Notification> {
    NOTIFICATION_TX.subscribe()
}

/// Publish a notification. Silently ignores having no active subscriber.
pub fn push(source: impl Into<String>, level: NotifyLevel, message: impl Into<String>) {
    let _ = NOTIFICATION_TX.send(Notification {
        source: source.into(),
        level,
        message: message.into(),
    });
}
