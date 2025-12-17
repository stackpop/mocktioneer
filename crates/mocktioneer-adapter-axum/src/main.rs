use mocktioneer_core::MocktioneerApp;

fn main() {
    if let Err(err) =
        edgezero_adapter_axum::run_app::<MocktioneerApp>(include_str!("../../../edgezero.toml"))
    {
        eprintln!("mocktioneer adapter failed: {err}");
        std::process::exit(1);
    }
}
