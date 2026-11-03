# Private Maintenance scope (5d.3x)

`RestoreMaintenanceScope<'s>` is a private owner in restore_context.rs. The local
scope in `with_active` owns the same real Maintenance previously held directly
by its local variable. Its sole accessor lends `&Maintenance<'s>` with a borrow
bounded by the scope; it exposes no Connection, file, reset or authority token.
There is no custom Drop, allocation, retention, terminal activation or abort.

Acquisition still follows require_supported and precedes ensure_usable,
generation checks, candidate verification, ActiveDatabase opening and protection
creation. Context/session/protection borrow the same guard. The context is still
consumed by the same higher-ranked callback; callback signatures are unchanged.
Normal field Drop releases Maintenance at the previous local guard boundary,
after later local context resources on the ordinary paths. Function-argument and
early-return cleanup order is not rearranged. No SQL, SQLite-close algorithm,
classification, recovery, generation or Ready logic changes.

On unwind, the existing Maintenance guard still drops normally. Its admission
flag is released and its generation RwLock may be poisoned; this is the existing
conservative coordination error, not a new automatic recovery or resumed Ready.
Inner resource retention continues to use existing owners. This wrapper neither
retains actual Maintenance permanently nor repairs an unsafe active episode.

Existing context tests cover normal return, controlled callback error, queued
Operation blocking/release and existing recovery/generation behavior. One new
runtime test compares actual with_active unwind against direct existing guard
unwind, requires reaching the context callback and its exact panic, checks the
same coordination errors afterwards and unchanged private journal bytes.
The existing compiler harness checks projected actual callback signatures and
ownership definitions with adjacent I/O stubs; it is not a new runtime proof of
the scope itself. The real context tests execute the changed with_active body.

This is an ownership anchor only. It is not a terminal safety contract or
production-ready restore: no writer, VFS, close proof or Ready path is introduced.
Future active ownership, process-wide descriptor controls, checked writer close,
terminal policy and a mandatory classification gate remain separate work.
