# Private live B/C ownership (5d.4c)

Previously ActiveCloseParts contained consecutive fields `incoming:
PreparedRestore` (B) and `protection: Option<VerifiedProtection>` (C). It now owns
one private `sources: LiveSources` in exactly their former position. LiveSources
contains precisely those same fields in the same order: incoming, protection.
It has no methods, custom Drop, Clone/Copy/Serde or independent authority.

The sole ActiveCloseParts construction moves the original incoming/protection
values from the consumed RestoreContext directly into LiveSources. No new open,
close, descriptor duplication, allocation or callback is introduced by this move.
P, binding, Maintenance reference, generation and original session remain in
ActiveCloseParts; the session is not exposed through LiveSources.

Effective ordinary destruction order is unchanged:

    before: active -> incoming -> protection -> maintenance/generation/etc.
                   -> pending -> binding -> session
    after:  active -> sources.incoming -> sources.protection
                   -> maintenance/generation/etc. -> pending -> binding -> session

Rust destroys struct fields in declaration order. Neither LiveSources nor
ActiveCloseParts has custom Drop; the new nesting introduces no action between
B and C. CloseOwner's existing whole-before/after retention during unwind is
unchanged. The existing resource types retain their original controlled SQLite
shutdown, BUSY retention and staging-cleanup behavior. This grouping does not
change any consuming into_parts or try_close implementation or call order.

Only necessary post-A-close field accesses in check_after, preflight and existing
tests follow the new sources path. Earlier WriteParts/RestoreContext accesses
are unchanged. Existing ownership/session/P/identity tests, staging-before-
reservation tests, BUSY/error/unwind tests and compiler probes are reused;
no parallel test resource model or new runtime observation is needed.

LiveSources is a private transfer unit for living sources, NOT a close proof.
B/C may still have live SQLite connections. Actual writer ownership, controlled
backup_finish, B/C close-ledger integration, terminal production policy,
process-wide POSIX descriptor control and a mandatory classification port remain
separate work. No writer, journal mutation, Ready or classify_session change is
introduced.
