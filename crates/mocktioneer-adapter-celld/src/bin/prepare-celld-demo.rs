use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use edgezero_core::app_config::{AppConfigLoadOptions, load_app_config_with_options};
use edgezero_core::blob_envelope::BlobEnvelope;
use mocktioneer_core::config::MocktioneerConfig;

const CONFIG_PATH: &str = "crates/mocktioneer-adapter-celld/mocktioneer-demo.toml";
const BUILD_PATH: &str = "crates/mocktioneer-adapter-celld/build";
const NORMAL_PROJECT: &str = "crates/mocktioneer-adapter-celld/celld/normal";
const DEMO_PROJECT: &str = "crates/mocktioneer-adapter-celld/celld/demo";
const WRAPPER_PATH: &str = "crates/mocktioneer-adapter-celld/celld-demo-wrapper.mjs";
const SEEDER_PATH: &str = "crates/mocktioneer-adapter-celld/demo-seeder.mjs";
const GENERATED_AT: &str = "2026-10-13T00:00:00Z";
const DEMO_CPM: f64 = 0.37_f64;

fn main() -> Result<(), Box<dyn Error>> {
    let build = Path::new(BUILD_PATH);
    if !build.is_dir() {
        return Err(format!(
            "{} is missing; run `worker-build --release` from crates/mocktioneer-adapter-celld first",
            build.display()
        )
        .into());
    }

    let normal_runtime = Path::new(NORMAL_PROJECT).join("runtime");
    let demo_runtime = Path::new(DEMO_PROJECT).join("runtime");
    copy_tree(build, &normal_runtime.join("build"))?;
    copy_tree(build, &demo_runtime.join("build"))?;
    fs::copy(WRAPPER_PATH, demo_runtime.join("celld-demo-wrapper.mjs"))?;
    fs::copy(SEEDER_PATH, demo_runtime.join("demo-seeder.mjs"))?;

    let mut options = AppConfigLoadOptions::default();
    options.env_overlay = false;
    let config = load_app_config_with_options::<MocktioneerConfig>(
        Path::new(CONFIG_PATH),
        "mocktioneer",
        &options,
    )?;
    if config.bid_cpm.to_bits() != DEMO_CPM.to_bits() {
        return Err(format!("the local celld fixture must set bid_cpm to {DEMO_CPM}").into());
    }

    let envelope = BlobEnvelope::new(serde_json::to_value(config)?, GENERATED_AT.to_owned());
    envelope.verify()?;
    let encoded = STANDARD.encode(serde_json::to_vec(&envelope)?);
    let dev_vars = format!("MOCKTIONEER_DEMO_SEED=1\nMOCKTIONEER_DEMO_CONFIG_BASE64={encoded}\n");
    fs::write(Path::new(DEMO_PROJECT).join(".dev.vars"), dev_vars)?;

    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry_result in fs::read_dir(source)? {
        let dir_entry = entry_result?;
        let source_path = dir_entry.path();
        let destination_path: PathBuf = destination.join(dir_entry.file_name());
        if dir_entry.file_type()?.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}
