//! V2 disk transition shared by production and tests. The durable target is never inferred or changed.
//! Only a fully verified, retired episode returns its held session to startup.
use super::*;
use crate::backup::{
    pre_restore::durability::{self, SyncPoint},
    restore::private,
};
use std::io;
type Outcome<T> = activation::Outcome<T>;
fn error(_: impl std::fmt::Debug) -> activation::Error {
    activation::Error::Verification
}
fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn point(name: &str) -> Outcome<()> {
    activation::point(name)?;
    #[cfg(test)]
    tests::checkpoint(name)?;
    Ok(())
}
// Metadata only: no extra active-database descriptor is opened/closed during
// SQLite verification. These checks do not promise protection against same-user ABA.
fn identity(path: &Path) -> Outcome<Option<(u64, u64)>> {
    if !regular(path).map_err(error)? {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(path)?;
        Ok(Some((m.dev(), m.ino())))
    }
    #[cfg(not(unix))]
    {
        Err(activation::Error::Binding)
    }
}
struct Record {
    root: PathBuf,
    marker: ImportV2,
    bytes: Vec<u8>,
    generation: u32,
    directory: File,
    marker_file: File,
    archive: File,
}
impl Record {
    fn load(session: &RecoverySession<'_, '_>) -> Outcome<Self> {
        let root = root(session).map_err(error)?;
        directory(&root.join("recovery"), false).map_err(error)?;
        let m = marker(&root).map_err(error)?;
        directory(&root.join("backups/activations"), true).map_err(error)?;
        if activation::present(&root.join("backups/activations").join(&m.operation_id))? {
            return Err(activation::Error::Binding);
        }
        environment(&root, &m).map_err(error)?;
        let archive_path = root.join("backups/pre-restore").join(&m.protection_file);
        if !regular(&archive_path).map_err(error)? {
            return Err(activation::Error::Verification);
        }
        let path = root.join("recovery").join(activation::MARKER);
        let mut bytes = Vec::new();
        File::open(&path)?
            .take(activation::LIMIT + 1)
            .read_to_end(&mut bytes)?;
        let result = Self {
            generation: session.startup_generation().map_err(error)?,
            directory: File::open(root.join("recovery"))?,
            marker_file: File::open(&path)?,
            archive: File::open(&archive_path)?,
            root,
            marker: m,
            bytes,
        };
        result.observe(session)?;
        Ok(result)
    }
    fn observe(&self, session: &RecoverySession<'_, '_>) -> Outcome<Assessment> {
        if root(session).map_err(error)? != self.root
            || session.startup_generation().map_err(error)? != self.generation
            || marker(&self.root).map_err(error)? != self.marker
            || !private::same_file(&self.directory, &self.root.join("recovery"))?
            || !private::same_file(
                &self.marker_file,
                &self.root.join("recovery").join(activation::MARKER),
            )?
            || !private::same_file(
                &self.archive,
                &self
                    .root
                    .join("backups/pre-restore")
                    .join(&self.marker.protection_file),
            )?
        {
            return Err(activation::Error::Binding);
        }
        let mut bytes = Vec::new();
        File::open(self.root.join("recovery").join(activation::MARKER))?
            .take(activation::LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes != self.bytes {
            return Err(activation::Error::Binding);
        }
        let a = classify(session, flag()).map_err(error)?;
        if a.protection != Protection::Verified
            || matches!(a.active, Active::Invalid | Active::Other)
            || matches!(a.new_candidate, Candidate::Invalid | Candidate::WrongProof)
            || matches!(
                a.previous_candidate,
                Candidate::Invalid | Candidate::WrongProof
            )
        {
            return Err(activation::Error::Verification);
        }
        Ok(a)
    }
    fn selected(&self, a: &Assessment) -> bool {
        matches!(
            (self.marker.target, a.active),
            (Target::New, Active::New | Active::Both) | (Target::Old, Active::Old | Active::Both)
        )
    }
    fn verify_selected(&self, session: &RecoverySession<'_, '_>) -> Outcome<()> {
        let a = self.observe(session)?;
        if !self.selected(&a)
            || (self.marker.target == Target::Old
                && !a.identical_content
                && a.new_candidate != Candidate::Verified)
        {
            return Err(activation::Error::Verification);
        }
        Ok(())
    }
}
fn sync_file(path: &Path) -> Outcome<()> {
    let f = File::open(path)?;
    durability::sync_opened(SyncPoint::FinalFileBarrier, &f)?;
    Ok(())
}
fn sync_dir(path: &Path) -> Outcome<()> {
    durability::sync(SyncPoint::Directory, path)?;
    Ok(())
}
// No classification or cached decision is accepted as an input capability.
// This function can only return after full selected-content verification and
// retirement; startup's existing one-use admission owns the remaining handoff.
pub(in crate::backup::restore_context::activation) fn complete_v2<'a, 's>(
    mut session: RecoverySession<'a, 's>,
) -> Outcome<RecoverySession<'a, 's>> {
    let r = Record::load(&session)?;
    let m = &r.marker;
    let active = r.root.join("mno.sqlite");
    let next = r.root.join("recovery").join(&m.candidate_file);
    let previous = r.root.join("recovery").join(&m.previous_candidate_file);
    // Make the selected target durable before any data-file operation.
    durability::sync_opened(SyncPoint::VerifiedFile, &r.marker_file)?;
    durability::sync_opened(SyncPoint::Directory, &r.directory)?;
    durability::sync_opened(SyncPoint::FinalFileBarrier, &r.marker_file)?;
    point("v2-target-durable")?;
    let mut a = r.observe(&session)?;
    if !r.selected(&a) {
        if a.active == Active::Missing && a.new_candidate != Candidate::Verified {
            return Err(activation::Error::Verification);
        }
        if m.target == Target::Old && a.previous_candidate == Candidate::Missing {
            previous_candidate::build(&mut session, flag()).map_err(error)?;
            a = r.observe(&session)?;
        }
        let (selected, other, selected_status, other_status) = match m.target {
            Target::New => (&next, &previous, a.new_candidate, a.previous_candidate),
            Target::Old => (&previous, &next, a.previous_candidate, a.new_candidate),
        };
        if selected_status != Candidate::Verified
            || (a.active == Active::Missing && other_status != Candidate::Verified)
        {
            return Err(activation::Error::Verification);
        }
        if a.active != Active::Missing && other_status == Candidate::Missing {
            let before_active = identity(&active)?;
            let before_selected = identity(selected)?;
            sync_file(&active)?;
            sync_file(selected)?;
            point("v2-before-park")?;
            if r.observe(&session)? != a
                || identity(&active)? != before_active
                || identity(selected)? != before_selected
                || identity(other)?.is_some()
            {
                return Err(activation::Error::Binding);
            }
            // Preserve the departed journal at its own marker-bound role name.
            previous_candidate::rename_exclusive(&active, other)?;
            point("v2-after-park")?;
            activation::sync_dir(&r.root, "v2-sync-park")?;
            sync_dir(&r.root.join("recovery"))?;
            sync_file(other)?;
            a = r.observe(&session)?;
        }
        // If the other candidate already existed, replacing the recognized
        // active inode is safe only because its exact content is preserved there.
        let expected_other = match m.target {
            Target::New => a.previous_candidate,
            Target::Old => a.new_candidate,
        };
        if expected_other != Candidate::Verified {
            return Err(activation::Error::Verification);
        }
        let before_active = identity(&active)?;
        let before_selected = identity(selected)?;
        sync_file(selected)?;
        sync_file(other)?;
        point("v2-before-install")?;
        if r.observe(&session)? != a
            || identity(&active)? != before_active
            || identity(selected)? != before_selected
        {
            return Err(activation::Error::Binding);
        }
        if before_active.is_none() {
            previous_candidate::rename_exclusive(selected, &active)?;
        } else {
            fs::rename(selected, &active)?;
        }
        point("v2-after-install")?;
        sync_dir(&r.root.join("recovery"))?;
        activation::sync_dir(&r.root, "v2-sync-install")?;
        sync_file(&active)?;
    }
    r.verify_selected(&session)?;
    point("v2-selected-verified")?;
    // Close is checked inside inspect/classify. All SQLite owners are gone
    // before syncing/retiring; the protection archive is never moved or erased.
    for path in [&active, &next, &previous] {
        if regular(path).map_err(error)? {
            sync_file(path)?;
        }
    }
    sync_file(&r.root.join("recovery").join(activation::MARKER))?;
    sync_dir(&r.root.join("recovery"))?;
    sync_dir(&r.root)?;
    activation::startup::check_v2_installation(&r.root, true)?;
    let archives = r.root.join("backups/activations");
    match fs::create_dir(&archives) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    directory(&archives, false).map_err(error)?;
    sync_dir(&r.root.join("backups"))?;
    let audit = archives.join(&m.operation_id);
    if activation::present(&audit)? {
        return Err(activation::Error::Binding);
    }
    point("v2-before-retire")?;
    r.verify_selected(&session)?;
    previous_candidate::rename_exclusive(&r.root.join("recovery"), &audit)?;
    point("v2-after-retire")?;
    activation::sync_dir(&archives, "v2-sync-retire")?;
    sync_dir(&r.root)?;
    sync_file(&active)?;
    session.ensure_current().map_err(error)?;
    point("v2-before-handoff")?;
    Ok(session)
}
#[cfg(test)]
mod tests;

// Read-only availability for the existing action command. Never a swap proof.
pub(super) fn available(session: &RecoverySession<'_, '_>) -> Outcome<()> {
    let r = Record::load(session)?;
    let a = r.observe(session)?;
    if r.selected(&a) {
        return r.verify_selected(session);
    }
    let (selected, other) = match r.marker.target {
        Target::New => (a.new_candidate, a.previous_candidate),
        Target::Old => (a.previous_candidate, a.new_candidate),
    };
    let build_old = r.marker.target == Target::Old && selected == Candidate::Missing;
    if (selected != Candidate::Verified && !build_old)
        || (a.active == Active::Missing && other != Candidate::Verified)
    {
        return Err(activation::Error::Verification);
    }
    Ok(())
}
