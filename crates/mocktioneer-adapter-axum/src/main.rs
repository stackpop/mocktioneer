use std::process;

use edgezero_adapter_axum::dev_server::run_app;
use mocktioneer_core::MocktioneerApp;

fn main() {
    if let Err(err) = run_app::<MocktioneerApp>(include_str!("../../../edgezero.toml")) {
        #[expect(
            clippy::print_stderr,
            reason = "startup-error path: logger may not be initialised yet"
        )]
        let () = eprintln!("mocktioneer adapter failed: {err}");
        process::exit(1);
    }
}
