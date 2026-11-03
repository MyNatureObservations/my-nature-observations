//! Explicit consent for an OPEN v2 episode only. This contract grants no write
//! or admission authority; its publisher must revalidate independently.
use super::*;

#[derive(Debug, PartialEq, Eq)]
enum Error {
    Observation(Blocked),
    ClosedEpisode,
    NotNew,
    UnsafeState,
    Stale,
    MissingDecision,
    NotConfirmed,
}
impl From<Blocked> for Error {
    fn from(value: Blocked) -> Self {
        Self::Observation(value)
    }
}
type Outcome<T> = std::result::Result<T, Error>;
#[derive(Debug, PartialEq, Eq)]
struct Binding {
    session_id: uuid::Uuid,
    directory_identity: (u64, u64),
    generation: u32,
    marker: ImportV2,
    active: Active,
}
#[derive(Debug)]
struct Preview(Binding);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserChoice {
    ReturnPrevious,
    KeepNew,
    Cancel,
}
#[derive(Debug, PartialEq, Eq)]
struct Confirmation {
    binding: Binding,
    choice: UserChoice,
}
// A borrowed record of validated intent, NOT a future action capability.
// No consumer can publish a marker or obtain Ready from this value.
#[derive(Debug, PartialEq, Eq)]
struct ValidatedDecision<'a>(&'a Confirmation);
impl Preview {
    // A future app boundary must call this only for the user's explicit choice
    // concerning this preview. There is no implicit/default confirmation.
    fn confirm(self, choice: UserChoice) -> Confirmation {
        Confirmation {
            binding: self.0,
            choice,
        }
    }
}
fn current(session: &RecoverySession<'_, '_>, flag: Arc<AtomicBool>) -> Outcome<Binding> {
    let generation = session
        .startup_generation()
        .map_err(|_| Blocked::Ownership)?;
    let directory_identity = session
        .directory_identity()
        .map_err(|_| Blocked::Ownership)?;
    let root = root(session)?;
    directory(&root.join("recovery"), false)?;
    let m = marker(&root)?;
    if m.target != Target::New {
        return Err(Error::NotNew);
    }
    // A retired episode is historical evidence, never a reusable live decision.
    directory(&root.join("backups/activations"), true)?;
    if activation::present(&root.join("backups/activations").join(&m.operation_id))
        .map_err(|_| Blocked::Io)?
    {
        return Err(Error::ClosedEpisode);
    }
    let observed = classify(session, flag.clone())?;
    let bad = |candidate| matches!(candidate, Candidate::Invalid | Candidate::WrongProof);
    if observed.protection != Protection::Verified
        || bad(observed.new_candidate)
        || bad(observed.previous_candidate)
        || !matches!(observed.active, Active::Old | Active::New | Active::Both)
        // If old is still active, the new content must be independently checked
        // in its candidate. A marker hash alone is not verified new content.
        || (observed.active == Active::Old && observed.new_candidate != Candidate::Verified)
    {
        return Err(Error::UnsafeState);
    }
    if marker(&root)? != m {
        return Err(Error::Stale);
    }
    session
        .ensure_publication_phase()
        .map_err(|_| Blocked::Ownership)?;
    cancelled(&flag)?;
    Ok(Binding {
        session_id: session.session_id(),
        directory_identity,
        generation,
        marker: m,
        active: observed.active,
    })
}
fn preview(session: &RecoverySession<'_, '_>, flag: Arc<AtomicBool>) -> Outcome<Preview> {
    current(session, flag).map(Preview)
}
fn validate<'a>(
    session: &RecoverySession<'_, '_>,
    decision: Option<&'a Confirmation>,
    flag: Arc<AtomicBool>,
) -> Outcome<ValidatedDecision<'a>> {
    let decision = decision.ok_or(Error::MissingDecision)?;
    if decision.choice != UserChoice::ReturnPrevious {
        return Err(Error::NotConfirmed);
    }
    // Read and verify live state again. The preview is never action evidence.
    if current(session, flag)? != decision.binding {
        return Err(Error::Stale);
    }
    Ok(ValidatedDecision(decision))
}
#[cfg(test)]
mod tests;

mod publication;

// The UI binding omits session_id deliberately: status cannot retain a borrowed
// session across IPC. The action opens its own session, compares a fresh preview,
// then creates and revalidates the actual decision inside that SAME held session.
fn preview_key(preview: &Preview) -> String {
    use sha2::{Digest, Sha256};
    let b = &preview.0;
    let mut hash = Sha256::new();
    hash.update(b"mno-v2-previous-confirmation-v1\0");
    hash.update(b.directory_identity.0.to_be_bytes());
    hash.update(b.directory_identity.1.to_be_bytes());
    hash.update(b.generation.to_be_bytes());
    hash.update(serde_json::to_vec(&b.marker).expect("inert marker serializes"));
    hash.update(format!("{:?}", b.active).as_bytes());
    format!("{:x}", hash.finalize())
}
pub(super) fn confirmation_key(session: &RecoverySession<'_, '_>) -> Option<String> {
    preview(session, Arc::new(AtomicBool::new(false)))
        .ok()
        .map(|p| preview_key(&p))
}
pub(super) fn publish_confirmed(
    session: &mut RecoverySession<'_, '_>,
    key: Option<&str>,
) -> activation::Outcome<()> {
    let flag = Arc::new(AtomicBool::new(false));
    let preview = preview(session, flag.clone()).map_err(|_| activation::Error::Verification)?;
    if key != Some(preview_key(&preview).as_str()) {
        return Err(activation::Error::Binding);
    }
    let confirmation = preview.confirm(UserChoice::ReturnPrevious);
    validate(session, Some(&confirmation), flag.clone())
        .map_err(|_| activation::Error::Verification)?;
    publication::publish(session, Some(&confirmation), flag)
        .map_err(|_| activation::Error::Verification)
}
