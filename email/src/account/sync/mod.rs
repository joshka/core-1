//! Module dedicated to account synchronization.
//!
//! The core concept of this module is the [`AccountSyncBuilder`],
//! which allows you to synchronize folders and emails for a given
//! account using a Maildir backend.

pub mod config;

use advisory_lock::{AdvisoryFileLock, FileLockError, FileLockMode};
use futures::{stream::FuturesUnordered, StreamExt};
use log::{debug, error, info};
use std::{
    collections::{HashMap, HashSet},
    env, fmt,
    fs::OpenOptions,
    io,
    sync::Arc,
};
use thiserror::Error;

use crate::{
    account::config::AccountConfig,
    backend::{Backend, BackendBuilder, BackendContextBuilder},
    email::sync::{
        EmailSyncCache, EmailSyncCacheHunk, EmailSyncCachePatch, EmailSyncHunk, EmailSyncPatch,
        EmailSyncPatchManager,
    },
    envelope::{get::maildir::GetEnvelopeMaildir, list::maildir::ListEnvelopesMaildir},
    flag::{add::maildir::AddFlagsMaildir, set::maildir::SetFlagsMaildir},
    folder::{
        add::maildir::AddFolderMaildir,
        delete::maildir::DeleteFolderMaildir,
        expunge::maildir::ExpungeFolderMaildir,
        list::maildir::ListFoldersMaildir,
        sync::{
            FolderName, FolderSyncCache, FolderSyncCacheHunk, FolderSyncHunk,
            FolderSyncPatchManager, FolderSyncPatches, FolderSyncStrategy, FoldersName,
        },
    },
    maildir::{config::MaildirConfig, MaildirSessionBuilder, MaildirSessionSync},
    message::{
        add_raw_with_flags::maildir::AddRawMessageWithFlagsMaildir,
        move_::maildir::MoveMessagesMaildir, peek::maildir::PeekMessagesMaildir,
    },
    Result,
};

/// Errors related to account synchronization.
#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot synchronize account {0}: synchronization not enabled")]
    SyncAccountNotEnabledError(String),
    #[error("cannot synchronize account {1}: cannot open lock file")]
    SyncAccountOpenLockFileError(#[source] io::Error, String),
    #[error("cannot synchronize account {1}: cannot lock process")]
    SyncAccountLockFileError(#[source] FileLockError, String),
    #[error("cannot synchronize account {1}: cannot unlock process")]
    SyncAccountUnlockFileError(#[source] FileLockError, String),
}

/// The synchronization destination.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum Destination {
    /// An item needs to be synchronized to the local Maildir.
    Local,

    /// An item needs to be synchronized remotely.
    Remote,
}

impl fmt::Display for Destination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Remote => write!(f, "remote"),
        }
    }
}

/// Alias for the source destination.
pub type Source = Destination;

/// Alias for the target destination.
pub type Target = Destination;

/// The backend synchronization progress event.
///
/// Represents all the events that can be triggered during the backend
/// synchronization process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountSyncProgressEvent {
    BuildFolderPatch,
    GetLocalCachedFolders,
    GetLocalFolders,
    GetRemoteCachedFolders,
    GetRemoteFolders,
    ApplyFolderPatches(FolderSyncPatches),
    ApplyFolderHunk(FolderSyncHunk),

    BuildEnvelopePatch(FoldersName),
    EnvelopePatchBuilt(FolderName, EmailSyncPatch),
    GetLocalCachedEnvelopes,
    GetLocalEnvelopes,
    GetRemoteCachedEnvelopes,
    GetRemoteEnvelopes,
    ApplyEnvelopePatches(HashMap<FolderName, EmailSyncPatch>),
    ApplyEnvelopeHunk(EmailSyncHunk),
    ApplyEnvelopeCachePatch(EmailSyncCachePatch),

    ExpungeFolders(FoldersName),
    FolderExpunged(FolderName),
}

impl fmt::Display for AccountSyncProgressEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuildFolderPatch => write!(f, "Building folders diff patch"),
            Self::GetLocalCachedFolders => write!(f, "Getting local cached folders"),
            Self::GetLocalFolders => write!(f, "Getting local folders"),
            Self::GetRemoteCachedFolders => write!(f, "Getting remote cached folders"),
            Self::GetRemoteFolders => write!(f, "Getting remote folders"),
            Self::ApplyFolderPatches(patches) => {
                let x = patches.values().fold(0, |sum, patch| sum + patch.len());
                let y = patches.len();
                write!(f, "Processing {x} patches of {y} folders")
            }
            Self::ApplyFolderHunk(hunk) => write!(f, "{hunk}"),
            Self::BuildEnvelopePatch(folders) => {
                let n = folders.len();
                write!(f, "Building envelopes diff patch for {n} folders")
            }
            Self::EnvelopePatchBuilt(folder, patch) => {
                let n = patch.iter().fold(0, |sum, patch| sum + patch.len());
                write!(f, "Built {n} envelopes diff patch for folder {folder}")
            }
            Self::GetLocalCachedEnvelopes => write!(f, "Getting local cached envelopes"),
            Self::GetLocalEnvelopes => write!(f, "Getting local envelopes"),
            Self::GetRemoteCachedEnvelopes => write!(f, "Getting remote cached envelopes"),
            Self::GetRemoteEnvelopes => write!(f, "Getting remote envelopes"),
            Self::ApplyEnvelopePatches(_patches) => {
                write!(f, "Processing envelope patches")
            }
            Self::ApplyEnvelopeHunk(hunk) => write!(f, "{hunk}"),
            Self::ApplyEnvelopeCachePatch(_patch) => {
                write!(f, "Processing envelope cache patch")
            }
            Self::ExpungeFolders(folders) => write!(f, "Expunging {} folders", folders.len()),
            Self::FolderExpunged(folder) => write!(f, "Folder {folder} successfully expunged"),
        }
    }
}

/// The account synchronization report.
///
/// Gathers folder and email synchronization reports in one unique
/// report.
#[derive(Debug, Default)]
pub struct AccountSyncReport {
    pub folders: FoldersName,
    pub folders_patch: Vec<(FolderSyncHunk, Option<crate::Error>)>,
    pub folders_cache_patch: (Vec<FolderSyncCacheHunk>, Option<crate::Error>),
    pub emails_patch: Vec<(EmailSyncHunk, Option<crate::Error>)>,
    pub emails_cache_patch: (Vec<EmailSyncCacheHunk>, Option<crate::Error>),
}

/// The account synchronization progress callback.
#[derive(Clone)]
pub struct AccountSyncProgress(Arc<dyn Fn(AccountSyncProgressEvent) -> Result<()> + Send + Sync>);

impl Default for AccountSyncProgress {
    fn default() -> Self {
        Self::new(|_evt| Ok(()))
    }
}

impl AccountSyncProgress {
    pub fn new(f: impl Fn(AccountSyncProgressEvent) -> Result<()> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn emit(&self, evt: AccountSyncProgressEvent) {
        debug!("emitting sync progress event {evt:?}");
        if let Err(err) = (self.0)(evt.clone()) {
            debug!("error while emitting backend sync event {evt:?}, skipping it");
            debug!("{err:?}");
        }
    }
}

/// The account synchronization builder.
///
/// This is not really a builder since there is no `build()` function,
/// but it follows the builder pattern. When all the options are set
/// up, `sync()` synchronizes the current account locally, using the
/// given remote builder.
pub struct AccountSyncBuilder<B: BackendContextBuilder> {
    remote_builder: BackendBuilder<B>,
    on_progress: AccountSyncProgress,
    folders_strategy: FolderSyncStrategy,
    dry_run: bool,
}

impl<'a, B: BackendContextBuilder + 'static> AccountSyncBuilder<B> {
    /// Creates a new account synchronization builder.
    pub async fn new(remote_builder: BackendBuilder<B>) -> Result<AccountSyncBuilder<B>> {
        let folders_strategy = remote_builder
            .account_config
            .get_folder_sync_strategy()
            .clone();

        Ok(Self {
            remote_builder,
            on_progress: Default::default(),
            dry_run: Default::default(),
            folders_strategy,
        })
    }

    /// Sets the progress callback following the builder pattern.
    pub fn with_on_progress(
        mut self,
        f: impl Fn(AccountSyncProgressEvent) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        self.on_progress = AccountSyncProgress::new(f);
        self
    }

    /// Sets the dry run flag following the builder pattern.
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Sets the sync folders strategy following the builder pattern.
    pub fn with_folders_strategy(mut self, strategy: FolderSyncStrategy) -> Self {
        self.folders_strategy = strategy;
        self
    }

    /// Sets the sync folders strategy following the builder pattern.
    ///
    /// Like `with_folders_strategy()`, but takes an optional strategy
    /// instead.
    pub fn with_some_folders_strategy(mut self, strategy: Option<FolderSyncStrategy>) -> Self {
        if let Some(strategy) = strategy {
            self.folders_strategy = strategy;
        }
        self
    }

    /// Synchronizes the current account locally, using a Maildir
    /// backend.
    ///
    /// Acts like a `build()` function in a regular builder pattern,
    /// except that the synchronizer builder is not consumed.
    pub async fn sync(&self) -> Result<AccountSyncReport> {
        let account = &self.remote_builder.account_config.name;
        info!("starting synchronization of account {account}");

        if !self.remote_builder.account_config.is_sync_enabled() {
            debug!("sync feature not enabled for account {account}, aborting");
            return Err(Error::SyncAccountNotEnabledError(account.clone()).into());
        }

        let lock_file_path = env::temp_dir().join(format!("himalaya-sync-{}.lock", account));
        debug!("locking sync file {lock_file_path:?}");

        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(lock_file_path)
            .map_err(|err| Error::SyncAccountOpenLockFileError(err, account.clone()))?;
        lock_file
            .try_lock(FileLockMode::Exclusive)
            .map_err(|err| Error::SyncAccountLockFileError(err, account.clone()))?;

        let sync_dir = self.remote_builder.account_config.get_sync_dir()?;

        debug!("initializing folder and envelope cache");
        let conn = &mut self.remote_builder.account_config.get_sync_db_conn()?;
        FolderSyncCache::init(conn)?;
        EmailSyncCache::init(conn)?;

        let local_builder = LocalBackendBuilder::new(
            self.remote_builder.account_config.clone(),
            MaildirConfig {
                root_dir: sync_dir.clone(),
            },
        );

        debug!("applying folder aliases to the folder sync strategy");
        let folders_strategy = match &self.folders_strategy {
            FolderSyncStrategy::All => FolderSyncStrategy::All,
            FolderSyncStrategy::Include(folders) => FolderSyncStrategy::Include(
                folders
                    .iter()
                    .map(|folder| {
                        Ok(self
                            .remote_builder
                            .account_config
                            .get_folder_alias(folder)?)
                    })
                    .collect::<Result<_>>()?,
            ),
            FolderSyncStrategy::Exclude(folders) => FolderSyncStrategy::Exclude(
                folders
                    .iter()
                    .map(|folder| {
                        Ok(self
                            .remote_builder
                            .account_config
                            .get_folder_alias(folder)?)
                    })
                    .collect::<Result<_>>()?,
            ),
        };

        self.on_progress
            .emit(AccountSyncProgressEvent::BuildFolderPatch);

        let folder_sync_patch_manager = FolderSyncPatchManager::new(
            &self.remote_builder.account_config,
            local_builder.clone(),
            self.remote_builder.clone(),
            &folders_strategy,
            self.on_progress.clone(),
            self.dry_run,
        );

        debug!("building folder sync patch");
        let folder_sync_patch = folder_sync_patch_manager.build_patches().await?;
        debug!("{folder_sync_patch:#?}");

        info!("applying folder sync patch");
        let folder_sync_report = folder_sync_patch_manager
            .apply_patches(folder_sync_patch)
            .await?;
        info!("{folder_sync_report:#?}");

        let folders = folder_sync_report.folders.clone();

        self.on_progress
            .emit(AccountSyncProgressEvent::BuildEnvelopePatch(
                folders.clone(),
            ));

        let envelope_sync_patch_manager = EmailSyncPatchManager::new(
            &self.remote_builder.account_config,
            local_builder.clone(),
            self.remote_builder.clone(),
            self.on_progress.clone(),
            self.dry_run,
        );

        debug!("building envelope sync patch");
        let envelope_sync_patches =
            FuturesUnordered::from_iter(folders.iter().map(|folder| async {
                let patch = envelope_sync_patch_manager
                    .build_patch(folder.clone())
                    .await?;
                Ok((folder.clone(), patch))
            }))
            .collect::<Vec<Result<_>>>()
            .await;
        let envelope_sync_patches = envelope_sync_patches
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let envelope_sync_patches = HashMap::from_iter(envelope_sync_patches);
        debug!("{envelope_sync_patches:#?}");

        let envelope_sync_patch = envelope_sync_patches
            .values()
            .cloned()
            .flatten()
            .collect::<HashSet<_>>();

        self.on_progress
            .emit(AccountSyncProgressEvent::ApplyEnvelopePatches(
                envelope_sync_patches,
            ));

        debug!("applying envelope sync patch");
        let envelope_sync_report = envelope_sync_patch_manager
            .apply_patch(conn, envelope_sync_patch)
            .await?;
        debug!("{envelope_sync_report:#?}");

        self.on_progress
            .emit(AccountSyncProgressEvent::ExpungeFolders(folders.clone()));

        debug!("expunging folders");
        FuturesUnordered::from_iter(folders.iter().map(|folder| async {
            local_builder
                .clone()
                .build()
                .await?
                .expunge_folder(folder)
                .await?;
            self.remote_builder
                .clone()
                .build()
                .await?
                .expunge_folder(folder)
                .await?;
            self.on_progress
                .emit(AccountSyncProgressEvent::FolderExpunged(folder.clone()));
            Ok(())
        }))
        .collect::<Vec<Result<()>>>()
        .await;

        debug!("unlocking sync file");
        lock_file
            .unlock()
            .map_err(|err| Error::SyncAccountUnlockFileError(err, account.clone()))?;

        debug!("building final sync report");
        let sync_report = AccountSyncReport {
            folders,
            folders_patch: folder_sync_report.patch,
            folders_cache_patch: folder_sync_report.cache_patch,
            emails_patch: envelope_sync_report.patch,
            emails_cache_patch: envelope_sync_report.cache_patch,
        };
        debug!("{sync_report:#?}");

        Ok(sync_report)
    }
}

#[derive(Clone)]
pub struct LocalBackendBuilder(BackendBuilder<MaildirSessionBuilder>);

impl LocalBackendBuilder {
    pub fn new(account_config: AccountConfig, maildir_config: MaildirConfig) -> Self {
        let session_builder = MaildirSessionBuilder::new(account_config.clone(), maildir_config);
        let backend_builder = BackendBuilder::new(account_config, session_builder)
            .with_add_folder(AddFolderMaildir::new)
            .with_list_folders(ListFoldersMaildir::new)
            .with_expunge_folder(ExpungeFolderMaildir::new)
            .with_delete_folder(DeleteFolderMaildir::new)
            .with_get_envelope(GetEnvelopeMaildir::new)
            .with_list_envelopes(ListEnvelopesMaildir::new)
            .with_add_flags(AddFlagsMaildir::new)
            .with_set_flags(SetFlagsMaildir::new)
            .with_peek_messages(PeekMessagesMaildir::new)
            .with_move_messages(MoveMessagesMaildir::new)
            .with_add_raw_message_with_flags(AddRawMessageWithFlagsMaildir::new);

        Self(backend_builder)
    }

    pub async fn build(self) -> Result<Backend<MaildirSessionSync>> {
        self.0.build().await
    }
}