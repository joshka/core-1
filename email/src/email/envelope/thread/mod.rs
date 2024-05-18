pub mod config;
#[cfg(feature = "imap")]
pub mod imap;
// #[cfg(feature = "maildir")]
// pub mod maildir;
// #[cfg(feature = "notmuch")]
// pub mod notmuch;

use async_trait::async_trait;
use petgraph::graphmap::DiGraphMap;

use super::{list::ListEnvelopesOptions, Envelopes, SingleId, ThreadedEnvelopes};
use crate::AnyResult;

#[async_trait]
pub trait ThreadEnvelopes: Send + Sync {
    /// Thread all available envelopes from the given folder matching
    /// the given pagination.
    async fn thread_envelopes(
        &self,
        folder: &str,
        opts: ListEnvelopesOptions,
    ) -> AnyResult<ThreadedEnvelopes>;

    async fn thread_envelope(&self, _folder: &str, _id: SingleId) -> AnyResult<ThreadedEnvelopes> {
        todo!()
    }
}
