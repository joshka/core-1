use async_trait::async_trait;
use log::info;
use maildirpp::Maildir;
use std::path::PathBuf;
use thiserror::Error;

use crate::{
    folder::FolderKind,
    maildir::{self, MaildirContextSync},
    Result,
};

use super::AddFolder;

#[derive(Error, Debug)]
pub enum Error {
    #[error("cannot create maildir folder structure at {1}")]
    CreateFolderStructureError(#[source] maildirpp::Error, PathBuf),
}

pub struct AddMaildirFolder {
    ctx: MaildirContextSync,
}

impl AddMaildirFolder {
    pub fn new(ctx: impl Into<MaildirContextSync>) -> Self {
        Self { ctx: ctx.into() }
    }

    pub fn new_boxed(ctx: impl Into<MaildirContextSync>) -> Box<dyn AddFolder> {
        Box::new(Self::new(ctx))
    }
}

#[async_trait]
impl AddFolder for AddMaildirFolder {
    async fn add_folder(&self, folder: &str) -> Result<()> {
        info!("creating maildir folder {folder}");

        let ctx = self.ctx.lock().await;
        let config = &ctx.account_config;

        let path = if FolderKind::matches_inbox(folder) {
            ctx.session.path().join("cur")
        } else {
            let folder = config.get_folder_alias(folder);
            let folder = maildir::encode_folder(folder);
            ctx.session.path().join(format!(".{}", folder))
        };

        Maildir::from(path.clone())
            .create_dirs()
            .map_err(|err| Error::CreateFolderStructureError(err, path))?;

        Ok(())
    }
}
