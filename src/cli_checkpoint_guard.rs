//! Pre-mutation checkpoint exclusion, per remote-advanced-reconcile-v1.
//! The operation lock lives outside the checkpoint set so transport cannot
//! replace its inode while replacing a generation. It covers validation and
//! the mutation, then releases before post-commit publication. Explicit
//! recovery keeps its own validation rules.

use crate::cli::{
    Command, DataCommand, ManifestCommand, RecurrenceCommand, RefCommand, ResourceCommand,
    SyncCommand,
};
use crate::error::{Error, Result};
use crate::store::{WorkspaceConfig, WorkspaceState};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::time::{Duration, Instant};

pub(crate) struct OperationGuard(File);

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

/// None means read-only; Some(true) means an ordinary guarded mutation;
/// Some(false) means explicit recovery/initialization, serialized but with
/// its own validation. Exhaustive matches make new commands choose a policy.
fn policy(command: &Command) -> Option<bool> {
    match command {
        Command::Create(_)
        | Command::Update(_)
        | Command::Release(_)
        | Command::Close(_)
        | Command::Reopen(_)
        | Command::Resolve(_)
        | Command::Claim(_)
        | Command::Label(_)
        | Command::Dep(_) => Some(true),
        Command::Resource(ResourceCommand::Add(_) | ResourceCommand::Remove(_))
        | Command::Ref(RefCommand::Add(_) | RefCommand::Remove(_))
        | Command::Data(DataCommand::Set(_) | DataCommand::Remove(_))
        | Command::Recurrence(
            RecurrenceCommand::Create(_)
            | RecurrenceCommand::Delete(_)
            | RecurrenceCommand::Materialize(_),
        )
        | Command::Manifest(ManifestCommand::Commit(_)) => Some(true),
        Command::Sync(command) => match command {
            SyncCommand::Fork(_) => Some(true),
            SyncCommand::FlushOnly(opts) => opts.output.is_none().then_some(true),
            SyncCommand::ImportOnly(_) | SyncCommand::Reconcile(_) => Some(false),
            SyncCommand::Status(_) | SyncCommand::Diff(_) | SyncCommand::Bisect(_) => None,
        },
        Command::Init(_) | Command::Restore(_) | Command::Redact(_) => Some(false),
        Command::Doctor(opts) => {
            (opts.repair || (opts.starvation_recovery && opts.force)).then_some(true)
        }
        Command::Watchdog(opts) => (!opts.dry_run).then_some(true),
        Command::AnalyzeExclusion(opts) => opts.attach.is_some().then_some(true),
        Command::List(_)
        | Command::Show(_)
        | Command::Capabilities(_)
        | Command::Schema(_)
        | Command::Query(_)
        | Command::Changes(_)
        | Command::Why(_)
        | Command::Compare(_)
        | Command::Policy(_)
        | Command::Resource(ResourceCommand::List(_))
        | Command::Ref(RefCommand::List(_) | RefCommand::Find(_))
        | Command::Data(DataCommand::Get(_) | DataCommand::List(_))
        | Command::Recurrence(
            RecurrenceCommand::Show(_) | RecurrenceCommand::List(_) | RecurrenceCommand::History(_),
        )
        | Command::Manifest(ManifestCommand::DryRun(_)) => None,
    }
}

pub(crate) fn acquire(command: &Command) -> Result<Option<OperationGuard>> {
    // Init deliberately acts in cwd, even below a foreign ancestor store.
    // Its own native-store check must remain the authority for that path.
    if matches!(command, Command::Init(_)) {
        return Ok(None);
    }
    let Some(validate) = policy(command) else {
        return Ok(None);
    };
    let config = match WorkspaceConfig::probe()? {
        WorkspaceState::Ready(config) => config,
        // Preserve init/restore's native handling of absent and uninitialized stores.
        _ => return Ok(None),
    };
    let path = config.root.join(".beads/operation.lock");
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|msg| Error::Io {
            path: path.clone(),
            msg,
        })?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(Error::DatabaseBusy(
                        "workspace operation lock busy; retry after the current writer finishes"
                            .into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(msg) => return Err(Error::Io { path, msg }),
        }
    }
    let guard = OperationGuard(file);
    if validate {
        let _publication = crate::service::acquire_checkpoint_publication_lock(
            &config.root.join(".beads/checkpoint"),
        )?;
        let conn = rusqlite::Connection::open_with_flags(
            config.database_path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        crate::service::reconcile::require_writable(&conn, &config.root.join(".beads"))?;
    }
    Ok(Some(guard))
}

pub(crate) fn is_recovery(command: &Command) -> bool {
    matches!(
        command,
        Command::Restore(_) | Command::Redact(_) | Command::Sync(SyncCommand::ImportOnly(_))
    )
}
