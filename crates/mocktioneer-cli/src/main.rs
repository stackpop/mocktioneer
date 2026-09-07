//! Mocktioneer CLI — built on the `edgezero-cli` library.
//!
//! Reuses every built-in edgezero command and adds the **typed** `config`
//! arms parameterised over `MocktioneerConfig`, so `validator` rules run on
//! `config validate` / `config push` / `config diff`.

use clap::{Parser, Subcommand};
use edgezero_cli::args::{
    ActiveVersionArgs, AuthArgs, BuildArgs, ConfigDiffArgs, ConfigGcArgs, ConfigPushArgs,
    ConfigValidateArgs, DeployArgs, HealthcheckArgs, ProvisionArgs, RollbackArgs, ServeArgs,
};
use edgezero_cli::DiffExit;
use mocktioneer_core::config::MocktioneerConfig;

#[derive(Parser, Debug)]
#[command(name = "mocktioneer-cli", about = "mocktioneer edge CLI")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Resolve and print the currently-active service version — capture a
    /// rollback target before a deploy supersedes it.
    ActiveVersion(ActiveVersionArgs),
    /// Sign in / out / status against the adapter's native CLI.
    Auth(AuthArgs),
    /// Build the project for a target edge.
    Build(BuildArgs),
    /// Inspect or mutate the typed `mocktioneer.toml` app config.
    #[command(subcommand)]
    Config(MocktioneerConfigCmd),
    /// Deploy to a target edge.
    Deploy(DeployArgs),
    /// Probe a deployed version's health (Fastly staging lifecycle); exits
    /// non-zero when unhealthy after retries.
    Healthcheck(HealthcheckArgs),
    /// Create the platform resources backing the declared store ids.
    Provision(ProvisionArgs),
    /// Roll a service back to a previous version, or deactivate a staged
    /// version (Fastly staging lifecycle).
    Rollback(RollbackArgs),
    /// Run a local simulation (adapter-specific).
    Serve(ServeArgs),
}

/// Dispatches `validate`/`push`/`diff` to the typed entry points over
/// `MocktioneerConfig`.
#[derive(Subcommand, Debug)]
enum MocktioneerConfigCmd {
    /// Diff `mocktioneer.toml` against the live (or local-emulator) config
    /// store. Exits 0 (no changes), 1 (changes with `--exit-code`), 2 (error).
    Diff(ConfigDiffArgs),
    /// Reclaim orphaned chunk entries the config store leaked from prior
    /// oversized pushes. Store-derived and untyped (no `MocktioneerConfig`).
    /// A dry-run by default; deletes only with `--yes` + an explicit
    /// `--older-than`.
    Gc(ConfigGcArgs),
    /// Push `mocktioneer.toml` as a blob envelope to the adapter's config store.
    Push(ConfigPushArgs),
    /// Validate `edgezero.toml` + `mocktioneer.toml` against `MocktioneerConfig`.
    Validate(ConfigValidateArgs),
}

fn main() {
    use std::process;

    edgezero_cli::init_cli_logger();
    let result: Result<(), String> = match Args::parse().cmd {
        Cmd::ActiveVersion(args) => edgezero_cli::run_active_version(&args),
        Cmd::Auth(args) => edgezero_cli::run_auth(&args),
        Cmd::Build(args) => edgezero_cli::run_build(&args),
        Cmd::Config(MocktioneerConfigCmd::Diff(args)) => {
            // `run_config_diff_typed` returns `Result<DiffExit, String>`: a
            // non-zero exit code (1 = diff with `--exit-code`; 2 = unsupported)
            // exits the process directly, mirroring the generated template.
            match edgezero_cli::run_config_diff_typed::<MocktioneerConfig>(&args) {
                Ok(DiffExit { code: 0 }) => Ok(()),
                Ok(DiffExit { code }) => process::exit(code),
                Err(err) => Err(err),
            }
        }
        // `gc` inspects the store, not the typed config, so it uses the
        // untyped entry point (no `MocktioneerConfig` parameterisation).
        Cmd::Config(MocktioneerConfigCmd::Gc(args)) => edgezero_cli::run_config_gc(&args),
        Cmd::Config(MocktioneerConfigCmd::Push(args)) => {
            edgezero_cli::run_config_push_typed::<MocktioneerConfig>(&args)
        }
        Cmd::Config(MocktioneerConfigCmd::Validate(args)) => {
            edgezero_cli::run_config_validate_typed::<MocktioneerConfig>(&args)
        }
        Cmd::Deploy(args) => edgezero_cli::run_deploy(&args),
        Cmd::Healthcheck(args) => edgezero_cli::run_healthcheck(&args),
        Cmd::Provision(args) => edgezero_cli::run_provision(&args),
        Cmd::Rollback(args) => edgezero_cli::run_rollback(&args),
        Cmd::Serve(args) => edgezero_cli::run_serve(&args),
    };
    if let Err(err) = result {
        log::error!("[mocktioneer] {err}");
        // Exit 2 for all errors so `config diff` errors satisfy the "errors are
        // always ≥ 2" contract; push / validate are not 1-vs-2 sensitive.
        process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::CommandFactory as _;

    /// Catches clap wiring mistakes (duplicate short flags, shadowed
    /// subcommands, malformed `#[command]` attrs) that otherwise only surface
    /// when the binary is run.
    #[test]
    fn cli_definition_is_valid() {
        Args::command().debug_assert();
    }
}
