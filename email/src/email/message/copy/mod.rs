use async_trait::async_trait;

use crate::{envelope::Id, Result};

#[cfg(feature = "imap")]
pub mod imap;
pub mod maildir;

#[async_trait]
pub trait CopyMessages: Send + Sync {
    /// Copy emails from the given folder to the given folder
    /// matching the given id.
    async fn copy_messages(&self, from_folder: &str, to_folder: &str, id: &Id) -> Result<()>;
}
