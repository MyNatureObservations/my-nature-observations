# Shared recovery artifact policy – 4c.4b

`startup::artifacts` is the shared, read-only namespace policy used by normal
startup, retained recovery Evidence and post-close FileChecks. It is not a
verification proof or permission to mutate files. Recovery classification remains
macOS-only; Linux/Windows remain Unsupported while artifact detection blocks
normal startup on all platforms.

## Reserved root names

| Name | Normal startup | Recovery classification / FileChecks |
| --- | --- | --- |
| recovery | Any object blocks | Classification allows only the strictly verified P/empty layouts; post-close requires absence |
| pending-restore.json in data root | Any object blocks | Rejected; real P belongs inside recovery |
| .mno-restore-completion-v1.json | Any object blocks | Only the exact retained, strictly verified Q object may be accepted |
| mno.sqlite-journal / -wal / -shm | Any object blocks | Any object blocks; SQLite is never opened to decide whether recovery is needed |
| .mno-completion-* | Blocks | Blocks except the exact current publication temporary during its existing authorized check |
| .mno-init-* | Blocks | Blocks |
| .pending-* in data root | Blocks | Blocks; legitimate pending temporaries live inside recovery |
| .mno-restore-completion* other than the version-1 name | Blocks | Blocks, including unknown versions |

Reserved names and prefixes are matched using encoded bytes with ASCII-only
case-insensitive comparisons. Uppercase and mixed-case variants therefore block
on case-sensitive and case-insensitive volumes alike. This is intentional: on
common macOS volumes an uppercase orphan can be reached through its lowercase
name, so case-sensitive enumeration alone would miss a reserved artifact.
No Unicode normalization, accent removal or general Unicode case folding is used.
Lookalikes with non-ASCII letters and names outside the exact/prefix boundaries
remain unrelated (unless the filesystem itself resolves a fixed-name lookup as
an alias). For example `.mno-completionnotes`, `.mno-initiation`, recovery-notes,
`mno.sqlite-wal-notes` and `.mnö-completion-note` are not reserved by this policy.
The `.mno-restore-completion*` namespace remains deliberately reserved in full.

The current publication temporary exception uses EXACT encoded-name equality,
never ASCII-insensitive equality. A case-only rename is not automatically exempt,
even on a case-insensitive volume. Publication's existing retained inode/type/
single-link checks and no-clobber rename remain mandatory. Namespace matching is
not identity authorization. The publisher may clean its own original temporary
according to that existing ownership contract; it never cleans arbitrary orphans.

Canonical recovery/Q names keep their existing specialized layout/identity checks.
Noncanonical case aliases of those names are rejected by the root scan; root P
is always rejected in every ASCII case. Sidecars use the same ASCII helpers for
exact basename-plus-suffix recognition, additionally enumerating the parent so
uppercase sidecars cannot evade canonical lookup on case-sensitive volumes. This
scan is read-only, checks presence only and never follows matching entries.

The byte-based prefix predicates also reject invalid-UTF8 suffixes on platforms
where the filesystem permits them. These are explicitly reserved namespaces,
not a blanket ban on unrelated dotfiles or user files. Other root names remain
accepted, subject to existing first-install rules. No global scan of backups or
the system temporary directory is added. `mno-restore-*` private staging can
legitimately remain owned by closed capabilities and is not a new root recovery
marker. Finished protection backups are neither deleted nor treated as failures.

`unexpected_root_name` and `unexpected_root` are the single policy for root
pending and reserved prefixes. Evidence, FileChecks and startup all call it.
The versioned Q and recovery directory keep their separate existing strict
identity/contents/layout verification. The shared `sidecars` function and
SIDECAR_SUFFIXES are used by startup and classification, preserving startup's
specific sidecar error messages and classification's SqliteState error.

## Startup order and error handling

DirectoryLease is acquired exactly once. Before establishment publication or any
SQLite-open, checkpoint checks recovery/P/Q with symlink_metadata and enumerates
reserved root names. Existing sidecar checks follow. After establishment work,
startup repeats the complete recovery-name checkpoint and sidecar checks directly
before the initializer, with DirectoryLease identity checks surrounding the work.
This catches artifacts introduced between the first check and initialization for
both existing-database opening and first database creation.

Presence blocks irrespective of type: regular file, empty file, directory,
symlink (including dangling), reparse point or other object. Namespace enumeration
only reads names; matching objects are not opened or followed. Exact-name metadata
checks use no-follow metadata. Only NotFound means absence. Enumeration/metadata
errors are not safe absence. All fixed recovery names and all sidecar names are
checked even if another was present, so an I/O failure is not silently discarded.

Artifact presence or checkpoint I/O error returns RecoveryGate owning the original
lease. DatabaseState retains that immutable ownership and Recovery phase; journal,
backup and maintenance access remain blocked, generation unchanged. No initializer
runs and no replacement database is created. The existing test for a Q metadata
error now additionally verifies retained gating/lock ownership. Unrelated identity
or initialization failures retain their earlier fail-closed behavior.

No automatic parsing, cleanup, rollback, schema repair or SQLite recovery is
introduced. If a late check blocks after legitimate establishment publication,
that establishment marker may already exist, but no SQLite initializer has run.
A missing established database remains blocked regardless of the marker state.
A fresh database still requires the earlier exclusive first-initialization rules.

## Threat model and limits

DirectoryLease coordinates MNO processes; it is not protection against arbitrary
same-user external manipulation. Existing root/lock identity checks detect observed
replacement. Name enumeration and the later SQLite-open are separate system calls;
a transient external change between them cannot be excluded. This limitation is
unchanged. No filesystem sandbox or unsafe fallback is introduced. An orphan
reserved temporary now conservatively blocks startup even if it could have come
from an interrupted legitimate operation; no data-loss/corruption conclusion is
made from its presence.

## Regression coverage

New deterministic startup tests cover all reserved root entries and three
sidecars, unrelated files, symlinks/directories, absent established journals with
and without a marker, metadata failures at each fixed recovery name, enumeration
errors, late temporary creation during the second checkpoint on both open paths,
no initializer calls, unchanged file trees, retained locks and blocked operations.
Invalid-UTF8 prefix handling is tested directly because the local macOS filesystem
rejects creation of such names with EILSEQ. Existing subprocess startup tests cover
second-process exclusion and crash release. Existing classifier, FileChecks,
publication and final-verification tests exercise the common policy in recovery.

No Q deletion, new persistent marker, generation change, recovery-to-Ready
transition, production restore or automatic SQLite recovery is added.

### Case-regression coverage

Additional real-filesystem tests exercise uppercase/mixed root names, all three
sidecars, early gating before establishment, late gating on both initializer
paths, classification Uncertain, and post-close FileChecks rejection. Inputs and
process-lock/reservation/Recovery guarantees remain checked. Boundary/non-ASCII
negative controls are accepted without broad Unicode folding. A direct namespace
test renames a real current temporary only in case; a separate production-pipeline
test verifies canonical temporary publication succeeds while case-renamed
publication fails before Q publication, preserving journal, P and protection.
These tests work on case-sensitive volumes too; they do not rely on alias lookup
being supported. Existing retained-file identity regression tests are unchanged.
