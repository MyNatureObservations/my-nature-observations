//! Test-only extraction laboratory. These types never authorize disk mutation or Ready.
use super::super::super::resource_close::TestClosedFiles;
use super::*;
use crate::{
    backup::restore_context::close::TestContextFiles,
    database_state::{
        Maintenance,
        recovery_session::{RecoverySession, TestSession, TestSessionBinding},
    },
};
use classification::Access;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    Before,
    Unwrapped,
    ContextClosed,
    FilesClosed,
    Detached,
}
#[derive(Debug)]
enum Error {
    Binding,
    Check,
    Panic,
}
// Declaration order keeps all files before the session, including on unwind.
struct Done<'s> {
    _marker: Option<RetainedMarker>,
    _context: Option<TestContextFiles<'s>>,
    _files: TestClosedFiles,
    session: TestSession<'s>,
}
struct Running<'s>(Done<'s>);
struct Startup<'s>(Done<'s>);
// At every checkpoint the complete chain has one aggregate owner.
enum Stage<'m, 's> {
    Before(FinalFilesVerified<'m, 's>),
    Unwrapped {
        marker: Option<RetainedMarker>,
        context: Option<Box<ContextConnectionsClosed<'m, 's>>>,
        resources: ResourcesClosed<'m, 's>,
    },
    ContextClosed {
        marker: Option<RetainedMarker>,
        context: Option<TestContextFiles<'s>>,
        resources: ResourcesClosed<'m, 's>,
    },
    FilesClosed {
        marker: Option<RetainedMarker>,
        context: Option<TestContextFiles<'s>>,
        files: TestClosedFiles,
        session: RecoverySession<'m, 's>,
    },
    Detached(Done<'s>),
}
// Destructors supplied by a test must not unwind through resource retention.
// A secondary panic payload is quarantined: dropping it could recurse forever.
// This narrowly scoped leak is test-only and never leaks the recovery owner.
struct PanicPayload(Option<Box<dyn std::any::Any + Send>>);
impl Drop for PanicPayload {
    fn drop(&mut self) {
        if let Some(payload) = self.0.take() {
            if let Err(secondary) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(payload)))
            {
                let _ = Box::leak(secondary);
            }
        }
    }
}
struct Checkpoint<F>(Option<F>);
impl<F> Checkpoint<F> {
    fn dispose(&mut self) -> Option<PanicPayload> {
        self.0.take().and_then(|callback| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(callback)))
                .err()
                .map(|payload| PanicPayload(Some(payload)))
        })
    }
}
impl<F> Drop for Checkpoint<F> {
    fn drop(&mut self) {
        // Also protects early prepare errors and unwinding before run().
        drop(self.dispose());
    }
}
struct Failure<'m, 's> {
    _panics: Vec<PanicPayload>,
    _stage: Box<Stage<'m, 's>>,
    error: Error,
}
struct Owner<'m, 's> {
    stage: Option<Stage<'m, 's>>,
    binding: Option<TestSessionBinding<'s>>,
}
impl<'m, 's> Owner<'m, 's> {
    fn step(&mut self) {
        // No I/O, hooks, allocation or user callbacks between take and install.
        // Some invariants were established by private constructors/checks.
        let stage = self.stage.take().expect("transfer stage");
        self.stage = Some(match stage {
            Stage::Before(mut final_owner) => {
                let Retained { marker, mut all } =
                    final_owner.retained.take().expect("verified owner");
                match all.held.take().expect("combined owner") {
                    Held::Running { context, resources } => Stage::Unwrapped {
                        marker,
                        context: Some(context),
                        resources,
                    },
                    Held::Startup { resources } => Stage::Unwrapped {
                        marker,
                        context: None,
                        resources,
                    },
                }
            }
            Stage::Unwrapped {
                marker,
                context,
                resources,
            } => Stage::ContextClosed {
                marker,
                context: context.map(|c| (*c).into_test_files()),
                resources,
            },
            Stage::ContextClosed {
                marker,
                context,
                mut resources,
            } => {
                let (files, access) =
                    (*resources.closed.take().expect("closed resources")).into_test_parts();
                let Access::Session(session) = access else {
                    unreachable!("checked session origin")
                };
                Stage::FilesClosed {
                    marker,
                    context,
                    files,
                    session,
                }
            }
            Stage::FilesClosed {
                marker,
                context,
                files,
                session,
            } => Stage::Detached(Done {
                _marker: marker,
                _context: context,
                _files: files,
                session: session.into_test_session(self.binding.take().expect("checked binding")),
            }),
            Stage::Detached(done) => Stage::Detached(done),
        });
    }
}
enum Route<'r, 's> {
    Running(&'r Maintenance<'s>),
    Startup,
}
fn prepare<'m, 's>(
    proof: FinalFilesVerified<'m, 's>,
    route: Route<'_, '_>,
) -> Result<Owner<'m, 's>, Failure<'m, 's>> {
    // Every fallible binding/filesystem/cancellation check precedes first take.
    let binding = (|| {
        let all = &proof.retained().all;
        file_checks::check_retained(all, proof.retained().marker.as_ref().ok_or(Error::Binding)?)
            .map_err(|_| Error::Check)?;
        let view = all.held().resources().closed().view();
        let Access::Session(session) = view.access else {
            return Err(Error::Binding);
        };
        match (all.held(), route) {
            (Held::Running { .. }, Route::Running(m)) => {
                session.bind_context(m).map_err(|_| Error::Binding)?;
            }
            (Held::Startup { .. }, Route::Startup) => {
                session.startup_generation().map_err(|_| Error::Binding)?;
            }
            _ => return Err(Error::Binding),
        }
        session.test_transfer_binding().map_err(|_| Error::Binding)
    })();
    match binding {
        Ok(binding) => Ok(Owner {
            stage: Some(Stage::Before(proof)),
            binding: Some(binding),
        }),
        Err(error) => Err(Failure {
            _panics: Vec::new(),
            _stage: Box::new(Stage::Before(proof)),
            error,
        }),
    }
}
fn run<'m, 's>(
    mut owner: Owner<'m, 's>,
    mut checkpoint: Checkpoint<impl FnMut(Point) -> Result<(), Error>>,
) -> Result<Done<'s>, Failure<'m, 's>> {
    // Owner stays outside catch_unwind. A checkpoint never borrows out ownership.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        checkpoint.0.as_mut().expect("live callback")(Point::Before)?;
        for point in [
            Point::Unwrapped,
            Point::ContextClosed,
            Point::FilesClosed,
            Point::Detached,
        ] {
            owner.step();
            checkpoint.0.as_mut().expect("live callback")(point)?;
        }
        Ok(())
    }));
    // Dispose the callback while the aggregate owner is still installed. Retain
    // both possible payloads; their later Drop is independently protected.
    let callback_panic = checkpoint.dispose();
    let (mut error, mut panics) = match outcome {
        Ok(Ok(())) => (None, Vec::new()),
        Ok(Err(error)) => (Some(error), Vec::new()),
        Err(payload) => (Some(Error::Panic), vec![PanicPayload(Some(payload))]),
    };
    if let Some(payload) = callback_panic {
        panics.push(payload);
        error = Some(Error::Panic);
    }
    let stage = owner.stage.take().expect("retained complete stage");
    if let Some(error) = error {
        return Err(Failure {
            _panics: panics,
            _stage: Box::new(stage),
            error,
        });
    }
    let Stage::Detached(done) = stage else {
        unreachable!()
    };
    Ok(done)
}
impl<'m, 's> FinalFilesVerified<'m, 's> {
    fn transfer_running(
        self,
        m: &Maintenance<'_>,
        checkpoint: impl FnMut(Point) -> Result<(), Error>,
    ) -> Result<Running<'s>, Failure<'m, 's>> {
        let checkpoint = Checkpoint(Some(checkpoint));
        run(prepare(self, Route::Running(m))?, checkpoint).map(Running)
    }
    fn transfer_startup(
        self,
        checkpoint: impl FnMut(Point) -> Result<(), Error>,
    ) -> Result<Startup<'s>, Failure<'m, 's>> {
        let checkpoint = Checkpoint(Some(checkpoint));
        run(prepare(self, Route::Startup)?, checkpoint).map(Startup)
    }
}

#[path = "test_transfer_tests.rs"]
mod tests;
