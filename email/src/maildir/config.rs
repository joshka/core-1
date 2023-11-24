//! Module dedicated to the Maildir backend configuration.
//!
//! This module contains the configuration specific to the Maildir
//! backend.

use std::path::PathBuf;

/// The Maildir backend configuration.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct MaildirConfig {
    /// The Maildir root directory.
    ///
    /// The path should point to the root level of the Maildir
    /// directory (the one containing the `cur`, `new` and `tmp`
    /// folders). Path is shell-expanded, which means environment
    /// variables and tilde `~` are replaced by their values.
    pub root_dir: PathBuf,
}