# Borrowed retained snapshot file check (5d.4f)

`ClosedPreparedRestore::check_retained_file(&self) -> Result<(), PrepareError>`
is available only on macOS, inside the existing backup-internal visibility
boundary. It borrows the existing retained File, BackupManifest and PrivateDir.
It moves no resources and produces no authority/proof object.

It checks before hashing and again after a matching full SHA-256:
- the staging path denotes a directory rather than a symlink;
- retained fd and fixed staging database name denote regular files;
- the named database itself is not a symlink;
- both observations have exactly one hard link and manifest database_size;
- Unix same_file observes matching device/inode for fd and fixed name.

Hashing uses read_exact_at on the SAME retained File, bounded to manifest size.
It neither duplicates the descriptor nor changes its shared file offset. Metadata
checks use fstat/lstat/stat; the macOS branch of existing same_file does not open
a File. No SQLite connection, file open/close, fsync or byte mutation occurs in
the check. All metadata/read/hash mismatches return CandidateChanged, following
the existing source-check error style. Errors retain the borrowed owner.

The check compares current named identity to the still-owned file object; it
does not invent a previously recorded staging-directory identity or original ZIP
inode. Concurrent same-user replacements/mutations between observations remain
TOCTOU risks; neither path observation nor hashing is an atomic filesystem
snapshot. The manifest/retained-owner provenance still comes from preparation.
No Windows path-opening fallback or Linux/Windows recovery path is introduced.

The method does NOT prove SQLite-close. ClosedPreparedRestore is also retained
file data INSIDE a live PreparedRestore. Consumers requiring actual close must
obtain it from the real checked try_close transition; merely calling this method
is insufficient. It does not check live transaction state, SQL schema/integrity,
P/Q, session, Maintenance, Recovery, main-journal identity or backup-archive hash.
It is not a classification port or durable completion/admission authority.

Three narrow runtime tests use real candidate preparation and checked try_close:
success preserves file position/identity and staging until owner Drop; replacement,
symlink, directory and missing-name cases are rejected; size, same-size byte hash
and link-count changes are rejected. After each reversible failure, restoring the
private fixture makes the SAME owner pass again. Test-only filesystem mutations
use private staging files and are not a production journal-writing path.

No runtime test counts every OS syscall; the no-open/dup/close property is also
established by reviewing the method and its macOS helper implementation. No new
Drop or existing close behavior changes. B/C-state integration, writer-close,
terminal production policy and POSIX lock ownership remain separate work.
