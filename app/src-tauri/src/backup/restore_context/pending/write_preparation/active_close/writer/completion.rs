//! Private bridge to the existing completion protocol; no new disk format.
use super::*;
use crate::backup::restore_context::pending::classification::{self, RunningHandoff, Status};
use crate::backup::restore_context::{RestoreMaintenanceScope, close::ContextConnectionsClosed};

// These methods are internal and fail-stop after arming. On an ordinary failure
// the returned stage/error is still alive when abort is called; it is never
// dropped before abort, and no generic cleanup or recovery authority is added.
fn checked<T, E>(result: std::result::Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(_retained) => std::process::abort(),
    }
}
fn point(name: &str) {
    checked(classification::result::receipt::event(name));
}
// Keeps the original P descriptors and terminal policy across the entire bridge.
struct Bridge<'m> {
    policy: Option<&'m TerminalPolicy>,
    pending: Option<OwnedPending>,
}
impl Drop for Bridge<'_> {
    fn drop(&mut self) {
        if let Some(policy) = self.policy {
            policy.abort_if_armed();
        }
    }
}
// No borrowed Maintenance survives this detached result. The policy address is
// equality-only, never dereferenced; moving the scope rejects rather than admits.
// Abandonment aborts while the complete handoff and original P handles still live.
struct CompletedEpisode<'s> {
    handoff: Option<RunningHandoff<'s>>,
    _pending: Option<OwnedPending>,
    policy_identity: *const TerminalPolicy,
}
impl Drop for CompletedEpisode<'_> {
    fn drop(&mut self) {
        if self.handoff.is_some() {
            std::process::abort();
        }
    }
}
impl<'m, 's> PreparedWriteEpisode<'m, 's> {
    fn complete(self, cancel: Arc<AtomicBool>) -> CompletedEpisode<'s> {
        let mut owner = self.inputs.inputs.owner;
        let policy = checked(owner.terminal_policy.ok_or(()));
        if !policy.is_armed() || owner.before.is_some() {
            std::process::abort();
        }
        let parts = checked(owner.after.as_ref().ok_or(()));
        if parts.copy != CopyState::Verified
            || !matches!(parts.writer, WriterState::Closed { .. })
            || !matches!(parts.sources, SourcePhase::Closed { .. })
            || parts.pending.is_none()
            || parts.session.is_none()
            || parts.binding.is_none()
        {
            std::process::abort();
        }
        checked(check_abc_connections_closed(parts));
        point("episode-before-transfer");
        // All fallible validation precedes extraction. Every moved resource is
        // already SQLite-closed. No I/O, allocation or hook during this handoff.
        let mut bridge = Bridge {
            policy: Some(policy),
            pending: None,
        };
        let parts = owner.after.take().expect("checked episode");
        let WriterState::Closed { _anchor: anchor } = parts.writer else {
            unreachable!()
        };
        let ActiveState::Closed(active) = parts.active else {
            unreachable!()
        };
        let SourcePhase::Closed {
            incoming,
            protection,
            record,
        } = parts.sources
        else {
            unreachable!()
        };
        bridge.pending = parts.pending;
        let context = ContextConnectionsClosed::from_episode(
            active,
            incoming,
            protection,
            record,
            parts.binding.expect("checked binding"),
            parts.maintenance,
            anchor,
        );
        let session = parts.session.expect("checked session");
        // Only an empty CloseOwner is neutralized; policy remains Armed, retained
        // both by Bridge and by the outer RestoreMaintenanceScope.
        owner.terminal_policy = None;
        drop(owner);
        point("episode-after-transfer");
        let assessment = classification::classify_session(session, cancel);
        point("episode-before-proof");
        let expected = checked(parse(&bridge.pending.as_ref().expect("original P").bytes));
        // Classifier prefers Old when both content references are identical.
        // Old is acceptable ONLY when it is also the exact expected new content.
        if assessment.status() != Status::New
            && !(assessment.status() == Status::Old && expected.old_content == expected.new_content)
        {
            std::process::abort();
        }
        let proof = checked(assessment.into_completion_proof());
        let receipt = checked(proof.publish_receipt());
        let removed = checked(receipt.remove_pending());
        let resources = checked(removed.close_resources());
        let all = checked(resources.combine_context(context));
        let verified = checked(all.verify_final());
        let prepared = checked(verified.prepare_q_removal());
        let unlinked = checked(prepared.unlink_q());
        let disk = checked(unlinked.complete_disk());
        let handoff = checked(disk.into_running_handoff(parts.maintenance));
        point("episode-handoff-complete");
        let done = CompletedEpisode {
            handoff: Some(handoff),
            _pending: bridge.pending.take(),
            policy_identity: std::ptr::from_ref(policy),
        };
        // Transfer the local terminal obligation to CompletedEpisode. This does
        // NOT change TerminalPolicy, and cannot permit normal scope release.
        bridge.policy = None;
        done
    }
}
impl<'s> RestoreMaintenanceScope<'s> {
    fn finish_episode(&mut self, mut completed: CompletedEpisode<'s>) {
        if !std::ptr::eq(completed.policy_identity, &self.terminal_policy)
            || !self.terminal_policy.is_armed()
        {
            std::process::abort();
        }
        point("episode-before-admission");
        let handoff = completed.handoff.take().expect("one-use completed episode");
        // Existing admission alone performs fresh validation and publishes Ready.
        checked(self.maintenance.resume_verified(handoff));
        // No fallible operation between successful Ready and Finished.
        self.terminal_policy
            .phase
            .set(TerminalPhase::Finished as u8);
        point("episode-finished");
    }
}
#[cfg(test)]
mod tests;
