//! Mocktioneer CLI — built on the `edgezero-cli` library.
//!
//! Reuses every built-in edgezero command and adds the **typed** `config`
//! arms parameterised over `MocktioneerConfig`, so `validator` rules run on
//! `config validate` / `config push`.

use clap::{Parser, Subcommand};
use edgezero_cli::args::{
    AuthArgs, BuildArgs, ConfigPushArgs, ConfigValidateArgs, DeployArgs, ProvisionArgs, ServeArgs,
};
use mocktioneer_core::config::MocktioneerConfig;

#[derive(Parser, Debug)]
#[command(name = "mocktioneer-cli", about = "mocktioneer edge CLI")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Sign in / out / status against the adapter's native CLI.
    Auth(AuthArgs),
    /// Build the project for a target edge.
    Build(BuildArgs),
    /// Inspect or mutate the typed `mocktioneer.toml` app config.
    #[command(subcommand)]
    Config(MocktioneerConfigCmd),
    /// Deploy to a target edge.
    Deploy(DeployArgs),
    /// Create the platform resources backing the declared store ids.
    Provision(ProvisionArgs),
    /// Run a local simulation (adapter-specific).
    Serve(ServeArgs),
}

/// Dispatches `validate`/`push` to the typed entry points over
/// `MocktioneerConfig`.
#[derive(Subcommand, Debug)]
enum MocktioneerConfigCmd {
    /// Push `mocktioneer.toml` (flattened) to the adapter's config store.
    Push(ConfigPushArgs),
    /// Validate `edgezero.toml` + `mocktioneer.toml` against `MocktioneerConfig`.
    Validate(ConfigValidateArgs),
}

fn main() {
    use std::process;

    edgezero_cli::init_cli_logger();
    let result = match Args::parse().cmd {
        Cmd::Auth(args) => edgezero_cli::run_auth(&args),
        Cmd::Build(args) => edgezero_cli::run_build(&args),
        Cmd::Config(MocktioneerConfigCmd::Push(args)) => {
            edgezero_cli::run_config_push_typed::<MocktioneerConfig>(&args)
        }
        Cmd::Config(MocktioneerConfigCmd::Validate(args)) => {
            edgezero_cli::run_config_validate_typed::<MocktioneerConfig>(&args)
        }
        Cmd::Deploy(args) => edgezero_cli::run_deploy(&args),
        Cmd::Provision(args) => edgezero_cli::run_provision(&args),
        Cmd::Serve(args) => edgezero_cli::run_serve(&args),
    };
    if let Err(err) = result {
        log::error!("[mocktioneer] {err}");
        process::exit(1);
    }
}
