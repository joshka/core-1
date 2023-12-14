use process::Cmd;
use serde::{Deserialize, Serialize};

/// Watch hook configuration.
///
/// Each variant represent the action that should be done when a
/// change occurs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WatchHook {
    /// Execute the shell command.
    ///
    /// For now, command is executed without any parameter nor
    /// input. This may change in the future.
    Cmd(Cmd),

    /// Send a system notification using the given
    /// [`notify_rust::Notification`]-like configuration.
    Notify(WatchNotifyConfig),
}

/// The watch configuration of the notify hook variant.
///
/// The structure tries to match the [`notify_rust::Notification`] API
/// and may evolve in the future.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WatchNotifyConfig {
    /// The summary (or the title) of the notification.
    ///
    /// Accepted placeholders:
    ///  - "{id}": the id of the envelope
    ///  - "{subject}": the subject of the envelope
    ///  - "{sender}" either the sender name or the address
    ///  - "{sender.name}" the sender name or "unknown"
    ///  - "{sender.address}" the sender address
    ///  - "{recipient}" either the recipient name or the address
    ///  - "{recipient.name}" the recipient name or "unknown"
    ///  - "{recipient.address}" the recipient address
    pub summary: String,

    /// The body of the notification.
    ///
    /// Accepted placeholders:
    ///  - "{id}": the id of the envelope
    ///  - "{subject}": the subject of the envelope
    ///  - "{sender}" either the sender name or the address
    ///  - "{sender.name}" the sender name or "unknown"
    ///  - "{sender.address}" the sender address
    ///  - "{recipient}" either the recipient name or the address
    ///  - "{recipient.name}" the recipient name or "unknown"
    ///  - "{recipient.address}" the recipient address
    pub body: String,
}