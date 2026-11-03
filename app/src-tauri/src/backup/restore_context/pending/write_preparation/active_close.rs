//! Explicit A close and private protected B/C close; retains P and the session.
//! A uses the process-global default VFS, not an owned VFS registration.
use super::*;
use crate::backup::pre_restore::ProtectionRecord;
use crate::backup::restore::{ClosedPreparedRestore, PreparedCloseFailure};

// None of these fields grant SQL/write/cleanup/admission authority externally.
enum ActiveState {
    Live(ActiveDatabase),
    Closed(ActiveFiles),
}
// Plain B/C owner: declaration order preserves incoming-before-protection Drop.
// No separate authority, resource accessor or custom Drop.
struct LiveSources<'m, 's> {
    incoming: Option<PreparedRestore>,
    protection: Option<VerifiedProtection<'m, 's>>,
}
// Private B/C-close producers retain the whole terminal episode in every phase.
// Every produced phase requires real close/failure owners; none grants completion.
#[allow(dead_code)]
enum SourcePhase<'m, 's> {
    Live(LiveSources<'m, 's>),
    BFailed {
        incoming: PreparedCloseFailure,
        protection: VerifiedProtection<'m, 's>,
    },
    BClosed {
        // Complete at method boundaries. Empty slots exist only during armed C handoff.
        incoming: Option<ClosedPreparedRestore>,
        protection: Option<VerifiedProtection<'m, 's>>,
        record: Option<ProtectionRecord<'m, 's>>,
    },
    CFailed {
        incoming: ClosedPreparedRestore,
        protection: PreparedCloseFailure,
        record: ProtectionRecord<'m, 's>,
    },
    Closed {
        incoming: ClosedPreparedRestore,
        protection: ClosedPreparedRestore,
        record: ProtectionRecord<'m, 's>,
    },
}
impl<'m, 's> SourcePhase<'m, 's> {
    // Borrow only; no resource extraction or fallback from partial to Live.
    fn live(&self) -> std::result::Result<&LiveSources<'m, 's>, ActiveCloseError> {
        match self {
            Self::Live(sources) if sources.incoming.is_some() => Ok(sources),
            _ => Err(ActiveCloseError::Binding),
        }
    }
}
// Private storage, not an opening/writing capability. Only isolated tests populate
// Owned until a separately reviewed writer factory exists. CloseOwner aborts
// before any of these fields can drop once Opening has been entered.
#[allow(dead_code)]
enum WriterState {
    Unopened,
    Opening,
    Owned {
        connection: Option<rusqlite::Connection>,
        anchor: File,
        outcome: Option<std::result::Result<(), (rusqlite::Connection, rusqlite::Error)>>,
    },
    Closed {
        _anchor: File,
    },
}
// Episode-local progress only: neither completion nor permission to release Maintenance.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyState {
    NotStarted,
    InProgress,
    Failed,
    Verified,
}
struct ActiveCloseParts<'m, 's> {
    writer: WriterState,
    copy: CopyState,
    active: ActiveState,
    sources: SourcePhase<'m, 's>,
    maintenance: &'m Maintenance<'s>,
    generation: u32,
    _next_generation: u32,
    _claimed: bool,
    pending: Option<OwnedPending>,
    binding: Option<ContextSessionBinding<'m, 's>>,
    // Reservation drops only after every preceding resource owner.
    session: Option<RecoverySession<'m, 's>>,
}
// Shared B/C-close transfer storage. Global pre-unwind panic policy protects
// consuming call frames; this Drop protects abandoned local resource ownership.
#[allow(dead_code)]
struct BTransferGuard<'policy> {
    terminal_policy: &'policy TerminalPolicy,
    incoming: Option<PreparedRestore>,
    // Starts empty; close_b/close_c store their real consuming result here.
    // After incoming in field Drop order; armed Drop aborts before BOTH fields.
    outcome: Option<std::result::Result<ClosedPreparedRestore, PreparedCloseFailure>>,
}
impl Drop for BTransferGuard<'_> {
    fn drop(&mut self) {
        // Runs before automatic destruction of incoming; never disarms policy.
        self.terminal_policy.abort_if_armed();
    }
}
struct CloseOwner<'m, 's> {
    terminal_policy: Option<&'m TerminalPolicy>,
    before: Option<WriteOwner<'m, 's>>,
    after: Option<ActiveCloseParts<'m, 's>>,
}
impl Drop for CloseOwner<'_, '_> {
    fn drop(&mut self) {
        if let Some(policy) = self.terminal_policy {
            policy.abort_if_armed();
        }
        if std::thread::panicking() {
            std::mem::forget(self.before.take());
            std::mem::forget(self.after.take());
        }
    }
}
/// Only confirmed A-close can mint this owner. B/C are STILL live, not closed.
/// No target open, SQL accessor, Ready, generation or Q-cleanup authority.
pub(crate) struct WriteInputs<'m, 's> {
    owner: CloseOwner<'m, 's>,
}
/// Unopened resource aggregate, not a writer, lock-epoch or completion proof.
/// A future writer must CONSUME this whole owner and retain it through checked
/// writer close. No file/connection accessor or extraction path is provided.
/// Never-armed owners retain existing B/C shutdown and unwind retention.
/// Writer preparation or close_b arms the owner; dropping it then stays terminal.
pub(crate) struct WriteEpochInputs<'m, 's> {
    inputs: WriteInputs<'m, 's>,
}
/// Whole episode ownership, including dormant writer-close storage; no opening,
/// copying or completion authority is exposed.
pub(crate) struct PreparedWriteEpisode<'m, 's> {
    inputs: WriteEpochInputs<'m, 's>,
}
/// Entire original owner remains available on a rejected preparation.
pub(crate) struct EpisodePreparationFailure<'m, 's> {
    inputs: WriteEpochInputs<'m, 's>,
    error: PreflightError,
}
impl<'m, 's> EpisodePreparationFailure<'m, 's> {
    pub(crate) fn error(&self) -> &PreflightError {
        &self.error
    }
    /// Return the complete owner, never individual resources or authority.
    pub(crate) fn into_inputs(self: Box<Self>) -> WriteEpochInputs<'m, 's> {
        self.inputs
    }
}
impl std::fmt::Debug for EpisodePreparationFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpisodePreparationFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl<'m, 's> WriteEpochInputs<'m, 's> {
    /// Pre-lock only. The original CloseOwner guards self throughout verification.
    /// No resource split; success is NOT permission to open a writer.
    pub(crate) fn prepare_episode(
        self,
    ) -> std::result::Result<PreparedWriteEpisode<'m, 's>, Box<EpisodePreparationFailure<'m, 's>>>
    {
        match self.check_before_writer() {
            Ok(()) => Ok(PreparedWriteEpisode { inputs: self }),
            Err(error) => Err(Box::new(EpisodePreparationFailure {
                inputs: self,
                error,
            })),
        }
    }
}
/// Historical borrowed metadata only; not fresh verification or authority.
/// The callback's higher-ranked borrow prevents returning this view or its bytes.
pub(crate) struct EpochVerificationView<'v> {
    pending_bytes: &'v [u8],
    session_id: uuid::Uuid,
    operation_id: uuid::Uuid,
    generation: u32,
}
impl EpochVerificationView<'_> {
    pub(crate) fn pending_bytes(&self) -> &[u8] {
        self.pending_bytes
    }
    pub(crate) fn session_id(&self) -> uuid::Uuid {
        self.session_id
    }
    pub(crate) fn operation_id(&self) -> uuid::Uuid {
        self.operation_id
    }
    pub(crate) fn generation(&self) -> u32 {
        self.generation
    }
}
#[derive(Debug)]
pub(crate) enum PreflightError {
    Binding,
    Existing(ActiveCloseError),
    Io(io::Error),
}
impl WriteEpochInputs<'_, '_> {
    /// BEFORE any future writer lock episode only. Performs I/O, including
    /// temporary opens/closes and B/C descriptor duplication. Not authorization.
    pub(crate) fn check_before_writer(&self) -> std::result::Result<(), PreflightError> {
        preflight::check(self)
    }
    /// No I/O, resource extraction, allocation or fresh verification. None fails
    /// closed if the private aggregate invariant is ever broken.
    pub(crate) fn with_verification<R>(
        &self,
        inspect: impl for<'v> FnOnce(EpochVerificationView<'v>) -> R,
    ) -> Option<R> {
        let parts = self.inputs.owner.after.as_ref()?;
        let pending = parts.pending.as_ref()?;
        let session = parts.session.as_ref()?;
        Some(inspect(EpochVerificationView {
            pending_bytes: &pending.bytes,
            session_id: session.session_id(),
            operation_id: pending.operation,
            generation: parts.generation,
        }))
    }
}
impl<'m, 's> WriteInputs<'m, 's> {
    /// Infallible whole-owner move: no I/O, allocation, callback or field take.
    /// Does not authorize opening, writing, releasing recovery or changing phase.
    pub(crate) fn into_epoch_inputs(self) -> WriteEpochInputs<'m, 's> {
        WriteEpochInputs { inputs: self }
    }
}
#[derive(Debug)]
pub(crate) enum ActiveCloseError {
    Binding,
    Marker(MarkerError),
    Session(String),
    Sqlite(rusqlite::Error),
    BSourceClose, // Original error and live B remain in SourcePhase::BFailed.
    BFileCheck,   // SQLite closed, but retained-file verification failed.
    CSourceClose, // Original error and live C remain in CFailed.
    CFileCheck,   // Closed owns A/B/C close-underlay, not successful verification.
    CopySource,
    CopyStep(i32),
    CopyFinish(i32),
    CopyVerification,
    WriterClose, // Original Connection and error remain in WriterState::Owned.
    WriterOpenIo(io::Error),
    WriterOpenSqlite(rusqlite::Error),
    WriterIdentity,
    WriterCancelled,
    #[cfg(test)]
    Checkpoint,
}
pub(crate) struct ActiveCloseFailure<'m, 's> {
    owner: CloseOwner<'m, 's>,
    error: ActiveCloseError,
}
impl ActiveCloseFailure<'_, '_> {
    pub(crate) fn error(&self) -> &ActiveCloseError {
        &self.error
    }
}
impl std::fmt::Debug for ActiveCloseFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveCloseFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for WriteInputs<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteInputs").finish_non_exhaustive()
    }
}
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClosePoint {
    BeforeClose,
    OwnersMoved,
    AfterClose,
}

fn check_before(parts: &mut WriteParts<'_, '_>) -> std::result::Result<(), ActiveCloseError> {
    let session = parts.session.as_ref().ok_or(ActiveCloseError::Binding)?;
    parts
        .binding
        .as_ref()
        .ok_or(ActiveCloseError::Binding)?
        .check_session(session)
        .map_err(ActiveCloseError::Session)?;
    parts
        .pending
        .as_mut()
        .ok_or(ActiveCloseError::Binding)?
        .verify(&parts.context)
        .map_err(ActiveCloseError::Marker)?;
    parts
        .context
        .incoming
        .source()
        .map_err(|_| ActiveCloseError::Binding)?;
    parts
        .context
        .protection
        .as_ref()
        .ok_or(ActiveCloseError::Binding)?
        .candidate()
        .source()
        .map_err(|_| ActiveCloseError::Binding)?;
    Ok(())
}
// This is a file-only after-check over actual closed A resources. It creates no
// independent close proof and does not fabricate a live ActiveDatabase.
fn check_after(parts: &ActiveCloseParts<'_, '_>) -> std::result::Result<(), ActiveCloseError> {
    let sources = parts.sources.live()?;
    check_common_after(parts)?;
    sources
        .incoming
        .as_ref()
        .ok_or(ActiveCloseError::Binding)?
        .source()
        .map_err(|_| ActiveCloseError::Binding)?;
    sources
        .protection
        .as_ref()
        .ok_or(ActiveCloseError::Binding)?
        .candidate()
        .source()
        .map_err(|_| ActiveCloseError::Binding)?;
    Ok(())
}

// Borrowed observation of this episode only. No independent proof or release authority.
#[cfg(all(target_os = "macos", any(test, panic = "abort")))]
fn check_abc_connections_closed(
    parts: &ActiveCloseParts<'_, '_>,
) -> std::result::Result<(), ActiveCloseError> {
    if !matches!(&parts.active, ActiveState::Closed(_))
        || !matches!(&parts.sources, SourcePhase::Closed { .. })
        || !matches!(
            parts.writer,
            WriterState::Unopened | WriterState::Closed { .. }
        )
    {
        return Err(ActiveCloseError::Binding);
    }
    check_common_after(parts)
}

fn check_common_after(
    parts: &ActiveCloseParts<'_, '_>,
) -> std::result::Result<(), ActiveCloseError> {
    #[cfg(not(unix))]
    {
        let _ = parts;
        Err(ActiveCloseError::Binding)
    }
    #[cfg(unix)]
    {
        let ActiveState::Closed(active) = &parts.active else {
            return Err(ActiveCloseError::Binding);
        };
        let session = parts.session.as_ref().ok_or(ActiveCloseError::Binding)?;
        let binding = parts.binding.as_ref().ok_or(ActiveCloseError::Binding)?;
        binding
            .check_session(session)
            .map_err(ActiveCloseError::Session)?;
        let pending = parts.pending.as_ref().ok_or(ActiveCloseError::Binding)?;
        let observed = (
            identity(&pending.root_file).map_err(ActiveCloseError::Marker)?,
            identity(&pending.directory_file).map_err(ActiveCloseError::Marker)?,
            identity(&pending.marker_file).map_err(ActiveCloseError::Marker)?,
        );
        if pending.operation != parts.maintenance.operation_id()
            || pending.generation != parts.generation
            || parts.generation != parts.maintenance.generation()
            || binding.database_path() != active.path
            || binding.operation_id() != pending.operation
            || private::file_identity(&active.file).ok() != Some(active.identity)
            || !private::same_file(&active.file, &active.path).unwrap_or(false)
            || pending.identities.as_ref() != Some(&observed)
            || observed.0
                != identity(&open_plain(&pending.root, true).map_err(ActiveCloseError::Marker)?)
                    .map_err(ActiveCloseError::Marker)?
            || observed.1
                != identity(
                    &open_plain(&pending.root.join(DIRECTORY), true)
                        .map_err(ActiveCloseError::Marker)?,
                )
                .map_err(ActiveCloseError::Marker)?
            || !pending
                .retained_name_matches()
                .map_err(|error| match error {
                    // The old after-check rejected these types with Binding.
                    MarkerError::Invalid => ActiveCloseError::Binding,
                    error => ActiveCloseError::Marker(error),
                })?
        {
            return Err(ActiveCloseError::Binding);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            let mut bytes = vec![0; pending.bytes.len()];
            if pending
                .marker_file
                .metadata()
                .map_err(|e| ActiveCloseError::Marker(e.into()))?
                .len()
                != bytes.len() as u64
            {
                return Err(ActiveCloseError::Binding);
            }
            pending
                .marker_file
                .read_exact_at(&mut bytes, 0)
                .map_err(|e| ActiveCloseError::Marker(e.into()))?;
            if bytes != pending.bytes {
                return Err(ActiveCloseError::Binding);
            }
            let data = parse(&bytes).map_err(ActiveCloseError::Marker)?;
            let (volume, file) = active.identity.components();
            if data.database_identity != Identity::from_parts(volume, file)
                || data.restore_id != pending.operation.to_string()
                || data.directory_identity != observed.0
            {
                return Err(ActiveCloseError::Binding);
            }
        }
        Ok(())
    }
}

impl<'m, 's> PendingWrite<'m, 's> {
    pub(crate) fn close_active(
        self,
    ) -> std::result::Result<WriteInputs<'m, 's>, Box<ActiveCloseFailure<'m, 's>>> {
        self.close_active_inner(
            #[cfg(test)]
            &mut |_| Ok(()),
        )
    }
    fn close_active_inner(
        self,
        #[cfg(test)] checkpoint: &mut impl FnMut(ClosePoint) -> io::Result<()>,
    ) -> std::result::Result<WriteInputs<'m, 's>, Box<ActiveCloseFailure<'m, 's>>> {
        let terminal_policy = self
            .owner
            .0
            .as_ref()
            .expect("pending parts")
            .context
            .terminal_policy;
        let mut owner = CloseOwner {
            terminal_policy,
            before: Some(self.owner),
            after: None,
        };
        let result = (|| {
            check_before(
                owner
                    .before
                    .as_mut()
                    .expect("pending owner")
                    .0
                    .as_mut()
                    .expect("pending parts"),
            )?;
            #[cfg(test)]
            checkpoint(ClosePoint::BeforeClose).map_err(|_| ActiveCloseError::Checkpoint)?;
            // All fallible binding checks precede this contiguous transfer.
            // Ordinary field moves only: no callback, allocation, unsafe or Drop
            // of a live resource can occur before the new aggregate owns it.
            let WriteParts {
                context,
                pending,
                binding,
                session,
            } = owner
                .before
                .as_mut()
                .expect("owner")
                .0
                .take()
                .expect("parts");
            let RestoreContext {
                terminal_policy: _,
                maintenance,
                active,
                incoming,
                protection,
                generation,
                next_generation,
                claimed,
            } = context;
            owner.after = Some(ActiveCloseParts {
                writer: WriterState::Unopened,
                copy: CopyState::NotStarted,
                active: ActiveState::Live(active),
                sources: SourcePhase::Live(LiveSources {
                    incoming: Some(incoming),
                    protection,
                }),
                maintenance,
                generation,
                _next_generation: next_generation,
                _claimed: claimed,
                pending,
                binding,
                session,
            });
            drop(owner.before.take()); // Empty original owner; no resource is released.
            #[cfg(test)]
            checkpoint(ClosePoint::OwnersMoved).map_err(|_| ActiveCloseError::Checkpoint)?;
            let parts = owner.after.as_mut().expect("closing parts");
            let ActiveState::Live(active) = &mut parts.active else {
                unreachable!()
            };
            let files = active.close_inner().map_err(ActiveCloseError::Sqlite)?;
            // A's connection was consumed successfully; assigning drops only an
            // already emptied ActiveDatabase. Original ActiveFiles stay owned.
            parts.active = ActiveState::Closed(files);
            #[cfg(test)]
            checkpoint(ClosePoint::AfterClose).map_err(|_| ActiveCloseError::Checkpoint)?;
            check_after(parts)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(WriteInputs { owner }),
            Err(error) => Err(Box::new(ActiveCloseFailure { owner, error })),
        }
    }
}
// Production unwind builds have no consuming B-close entry. Tests use this
// SAME body only in subprocesses that explicitly install the real panic hook.
#[cfg(all(target_os = "macos", any(test, panic = "abort")))]
impl<'m, 's> PreparedWriteEpisode<'m, 's> {
    fn close_b(self) -> std::result::Result<Self, Box<ActiveCloseFailure<'m, 's>>> {
        let mut owner = self.inputs.inputs.owner;
        let result = (|| {
            let policy = owner.terminal_policy.ok_or(ActiveCloseError::Binding)?;
            let parts = owner.after.as_mut().ok_or(ActiveCloseError::Binding)?;
            match &parts.writer {
                WriterState::Unopened if policy.is_never_armed() => (),
                WriterState::Closed { .. } if policy.is_armed() => (),
                _ => return Err(ActiveCloseError::Binding),
            }
            if !matches!(&parts.active, ActiveState::Closed(_))
                || parts.sources.live()?.protection.is_none()
                || parts.maintenance.generation() != parts.generation
            {
                return Err(ActiveCloseError::Binding);
            }
            check_after(parts)?;
            let mut guard = BTransferGuard {
                terminal_policy: policy,
                incoming: None,
                outcome: None,
            };
            if matches!(parts.writer, WriterState::Unopened) {
                policy.arm_for_b_close();
            } // Otherwise continue the already armed, checked-writer-close episode.
            let SourcePhase::Live(live) = &mut parts.sources else {
                unreachable!()
            };
            std::mem::swap(&mut guard.incoming, &mut live.incoming);
            #[cfg(test)]
            b_close_probe::checkpoint(BClosePoint::BeforeConsume);
            guard.outcome = Some(guard.incoming.take().expect("validated B").try_close());
            #[cfg(test)]
            b_close_probe::checkpoint(BClosePoint::AfterOutcome);
            // Infallible contiguous handoff under the pre-unwind panic policy.
            // The old Live variant is emptied before assignment drops it.
            #[cfg(test)]
            let skip_handoff = b_close_probe::skip_handoff();
            #[cfg(not(test))]
            let skip_handoff = false;
            if !skip_handoff {
                parts.sources = match guard.outcome.take().expect("real B close result") {
                    Ok(incoming) => SourcePhase::BClosed {
                        incoming: Some(incoming),
                        protection: live.protection.take(),
                        record: None,
                    },
                    Err(incoming) => SourcePhase::BFailed {
                        incoming,
                        protection: live.protection.take().expect("validated C"),
                    },
                };
                #[cfg(test)]
                b_close_probe::checkpoint(BClosePoint::AfterPhase);
            }
            #[cfg(test)]
            b_close_probe::checkpoint(BClosePoint::BeforeEmptyCheck);
            assert!(guard.incoming.is_none() && guard.outcome.is_none());
            assert!(matches!(
                &parts.sources,
                SourcePhase::BClosed {
                    incoming: Some(_),
                    protection: Some(_),
                    record: None
                } | SourcePhase::BFailed { .. }
            ));
            #[cfg(test)]
            b_close_probe::checkpoint(BClosePoint::EmptyGuardVerified);
            // Only a borrowed policy reference and two EMPTY slots remain.
            // Skip this local aborting Drop AFTER the complete handoff. No heap
            // owner, fd, connection, C/P/session/M is leaked or disarmed here.
            // This is deliberately inline: no general finish/disarm API exists.
            std::mem::forget(guard);
            match &parts.sources {
                SourcePhase::BClosed { incoming, .. } => {
                    incoming
                        .as_ref()
                        .ok_or(ActiveCloseError::Binding)?
                        .check_retained_file()
                        .map_err(|_| ActiveCloseError::BFileCheck)?;
                    Ok(())
                }
                SourcePhase::BFailed { .. } => Err(ActiveCloseError::BSourceClose),
                _ => unreachable!(),
            }
        })();
        match result {
            Ok(()) => Ok(Self {
                inputs: WriteEpochInputs {
                    inputs: WriteInputs { owner },
                },
            }),
            Err(error) => Err(Box::new(ActiveCloseFailure { owner, error })),
        }
    }
    // Continues the SAME armed episode. No rearm, retry, or normal release.
    fn close_c(self) -> std::result::Result<Self, Box<ActiveCloseFailure<'m, 's>>> {
        let mut owner = self.inputs.inputs.owner;
        let result = (|| {
            let policy = owner.terminal_policy.ok_or(ActiveCloseError::Binding)?;
            if !policy.is_armed() {
                return Err(ActiveCloseError::Binding);
            }
            let parts = owner.after.as_mut().ok_or(ActiveCloseError::Binding)?;
            if !matches!(
                parts.writer,
                WriterState::Unopened | WriterState::Closed { .. }
            ) {
                return Err(ActiveCloseError::Binding);
            }
            let SourcePhase::BClosed {
                incoming: Some(b),
                protection: Some(c),
                record: None,
            } = &parts.sources
            else {
                return Err(ActiveCloseError::Binding);
            };
            check_common_after(parts)?;
            b.check_retained_file()
                .map_err(|_| ActiveCloseError::BFileCheck)?;
            c.candidate()
                .source()
                .map_err(|_| ActiveCloseError::CFileCheck)?;
            let mut guard = BTransferGuard {
                terminal_policy: policy,
                incoming: None,
                outcome: None,
            };
            let SourcePhase::BClosed {
                incoming,
                protection,
                record,
            } = &mut parts.sources
            else {
                unreachable!()
            };
            // Infallible ownership-only split under pre-unwind policy. No callback
            // or ordinary error return while tuple payloads are outside slots.
            let (candidate, binding) = protection.take().expect("validated C").into_parts();
            guard.incoming = Some(candidate);
            *record = Some(binding);
            #[cfg(test)]
            c_close_probe::checkpoint(BClosePoint::BeforeConsume);
            guard.outcome = Some(guard.incoming.take().expect("retained C").try_close());
            #[cfg(test)]
            c_close_probe::checkpoint(BClosePoint::AfterOutcome);
            #[cfg(test)]
            let skip_handoff = c_close_probe::skip_handoff();
            #[cfg(not(test))]
            let skip_handoff = false;
            if !skip_handoff {
                parts.sources = match guard.outcome.take().expect("real C close result") {
                    Ok(closed) => SourcePhase::Closed {
                        incoming: incoming.take().expect("retained closed B"),
                        protection: closed,
                        record: record.take().expect("retained C binding"),
                    },
                    Err(failure) => SourcePhase::CFailed {
                        incoming: incoming.take().expect("retained closed B"),
                        protection: failure,
                        record: record.take().expect("retained C binding"),
                    },
                };
                #[cfg(test)]
                c_close_probe::checkpoint(BClosePoint::AfterPhase);
            }
            #[cfg(test)]
            c_close_probe::checkpoint(BClosePoint::BeforeEmptyCheck);
            assert!(guard.incoming.is_none() && guard.outcome.is_none());
            assert!(matches!(
                &parts.sources,
                SourcePhase::Closed { .. } | SourcePhase::CFailed { .. }
            ));
            #[cfg(test)]
            c_close_probe::checkpoint(BClosePoint::EmptyGuardVerified);
            // Only the empty local guard is forgotten; outer policy stays Armed.
            std::mem::forget(guard);
            match &parts.sources {
                SourcePhase::Closed {
                    incoming,
                    protection,
                    ..
                } => {
                    check_abc_connections_closed(parts)?;
                    incoming
                        .check_retained_file()
                        .map_err(|_| ActiveCloseError::BFileCheck)?;
                    protection
                        .check_retained_file()
                        .map_err(|_| ActiveCloseError::CFileCheck)?;
                    Ok(())
                }
                SourcePhase::CFailed { .. } => Err(ActiveCloseError::CSourceClose),
                _ => unreachable!(),
            }
        })();
        match result {
            Ok(()) => Ok(Self {
                inputs: WriteEpochInputs {
                    inputs: WriteInputs { owner },
                },
            }),
            Err(error) => Err(Box::new(ActiveCloseFailure { owner, error })),
        }
    }
}
#[cfg(all(test, target_os = "macos"))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum BClosePoint {
    BeforeConsume,
    AfterOutcome,
    AfterPhase,
    BeforeEmptyCheck,
    EmptyGuardVerified,
}
#[cfg(all(test, target_os = "macos"))]
macro_rules! close_probe_module {
    ($name:ident) => {
        mod $name {
            use super::BClosePoint;
            use std::{cell::RefCell, io::Write, path::PathBuf};
            struct Config {
                root: PathBuf,
                panic_at: Option<BClosePoint>,
                rename: Option<(PathBuf, PathBuf)>,
                skip: bool,
            }
            thread_local! { static CONFIG: RefCell<Option<Config>> = const { RefCell::new(None) }; }
            pub(super) fn configure(
                root: PathBuf,
                panic_at: Option<BClosePoint>,
                rename: Option<(PathBuf, PathBuf)>,
                skip: bool,
            ) {
                CONFIG.with(|c| {
                    *c.borrow_mut() = Some(Config {
                        root,
                        panic_at,
                        rename,
                        skip,
                    })
                });
            }
            pub(super) fn skip_handoff() -> bool {
                CONFIG.with(|c| c.borrow().as_ref().is_some_and(|c| c.skip))
            }
            pub(super) fn checkpoint(point: BClosePoint) {
                CONFIG.with(|c| {
                    let mut config = c.borrow_mut();
                    let Some(config) = config.as_mut() else {
                        return;
                    };
                    let name = match point {
                        BClosePoint::BeforeConsume => "before-consume",
                        BClosePoint::AfterOutcome => "after-outcome",
                        BClosePoint::AfterPhase => "after-phase",
                        BClosePoint::BeforeEmptyCheck => "before-empty-check",
                        BClosePoint::EmptyGuardVerified => "empty-guard-verified",
                    };
                    writeln!(
                        std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(config.root.join("events"))
                            .unwrap(),
                        "{name}"
                    )
                    .unwrap();
                    if point == BClosePoint::AfterPhase {
                        if let Some((from, to)) = config.rename.take() {
                            std::fs::rename(from, to).unwrap();
                        }
                    }
                    if config.panic_at == Some(point) {
                        panic!("B-close checkpoint");
                    }
                });
            }
        }
    };
}
#[cfg(all(test, target_os = "macos"))]
close_probe_module!(b_close_probe);
#[cfg(all(test, target_os = "macos"))]
close_probe_module!(c_close_probe);

#[cfg(all(test, target_os = "macos"))]
mod tests;

mod preflight;

#[cfg(all(target_os = "macos", any(test, panic = "abort")))]
mod writer;
