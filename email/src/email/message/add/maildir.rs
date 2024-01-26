use async_trait::async_trait;
use log::info;
use thiserror::Error;

use crate::{envelope::SingleId, maildir::MaildirContextSync, Result};

use super::{AddMessage, Flags};

#[derive(Error, Debug)]
pub enum Error {
    #[error("cannot add maildir message to folder {1} with flags {2}")]
    StoreWithFlagsError(#[source] maildirpp::Error, String, Flags),
}

#[derive(Clone)]
pub struct AddMaildirMessage {
    pub ctx: MaildirContextSync,
}

impl AddMaildirMessage {
    pub fn new(ctx: &MaildirContextSync) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &MaildirContextSync) -> Box<dyn AddMessage> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &MaildirContextSync) -> Option<Box<dyn AddMessage>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl AddMessage for AddMaildirMessage {
    async fn add_message_with_flags(
        &self,
        folder: &str,
        raw_msg: &[u8],
        flags: &Flags,
    ) -> Result<SingleId> {
        info!("adding maildir message to folder {folder} with flags {flags}");

        let ctx = self.ctx.lock().await;
        let mdir = ctx.get_maildir_from_folder_name(folder)?;

        let id = mdir
            .store_cur_with_flags(raw_msg, &flags.to_mdir_string())
            .map_err(|err| Error::StoreWithFlagsError(err, folder.to_owned(), flags.clone()))?;

        Ok(SingleId::from(id))
    }
}