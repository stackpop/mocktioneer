use mocktioneer_core::MocktioneerApp;

fn main() {
    if let Err(err) =
        anyedge_adapter_axum::run_app::<MocktioneerApp>(include_str!("../../../anyedge.toml"))
    {
        eprintln!("mocktioneer adapter failed: {err}");
        std::process::exit(1);
    }
}
