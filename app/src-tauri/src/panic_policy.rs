// Install before Tauri, plugins, workers, or app data initialization.
// Do not chain diagnostics or cleanup into the panic path. Rust permits later
// hook replacement; dependency changes must preserve this process policy.
pub(super) fn install() {
    std::panic::set_hook(Box::new(|info| {
        #[cfg(debug_assertions)]
        eprintln!("MNO startup panic: {info}");
        #[cfg(not(debug_assertions))]
        let _ = info;
        std::process::abort();
    }));
}

#[cfg(all(test, unix))]
#[path = "panic_policy/tests.rs"]
mod tests;
