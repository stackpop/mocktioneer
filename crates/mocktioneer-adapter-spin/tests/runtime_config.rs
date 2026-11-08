//! Keeps the Spin runtime config pointed at the database that
//! `config push --adapter spin --local` writes.
//!
//! Spin keeps a `type = "spin"` key-value store in memory unless the runtime
//! config gives it a `path`. A store declared without one silently ignores
//! pushed config, and `spin up` answers every auction with
//! `503 config_out_of_date`. The CLI writes to the declared `path`, resolved
//! against `runtime-config.toml`, or to `.spin/sqlite_key_value.db` beside
//! `spin.toml` when none is set. Both files live in this crate's directory, so
//! pinning `path` to that file keeps the writer and the runtime on one database.

#![cfg(not(target_arch = "wasm32"))]

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use toml::{Table, Value};

    /// Where the CLI writes when no `path` is set, relative to `spin.toml`.
    const CLI_SQLITE_PATH: &str = ".spin/sqlite_key_value.db";

    fn read_table(file_name: &str) -> Table {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(file_name);
        let text = fs::read_to_string(&path).expect("read Spin manifest file");
        text.parse::<Table>().expect("parse Spin manifest file")
    }

    #[test]
    fn every_component_kv_store_persists_where_config_push_writes() {
        let spin = read_table("spin.toml");
        let runtime = read_table("runtime-config.toml");
        let stores = runtime
            .get("key_value_store")
            .and_then(Value::as_table)
            .expect("runtime-config.toml has a [key_value_store] table");

        let labels: Vec<&str> = spin
            .get("component")
            .and_then(Value::as_table)
            .expect("spin.toml has a [component] table")
            .values()
            .filter_map(|component| component.get("key_value_stores"))
            .flat_map(|declared| declared.as_array().expect("key_value_stores is an array"))
            .map(|label| label.as_str().expect("key_value_stores label is a string"))
            .collect();
        assert!(
            !labels.is_empty(),
            "spin.toml declares no key_value_stores to check"
        );

        for label in labels {
            let store = stores
                .get(label)
                .unwrap_or_else(|| panic!("runtime-config.toml has no [key_value_store.{label}]"));
            assert_eq!(
                store.get("type").and_then(Value::as_str),
                Some("spin"),
                "[key_value_store.{label}] must use the SQLite-backed `spin` type"
            );
            assert_eq!(
                store.get("path").and_then(Value::as_str),
                Some(CLI_SQLITE_PATH),
                "[key_value_store.{label}] needs `path` set to where `config push --adapter spin --local` writes; without it Spin keeps the store in memory"
            );
        }
    }
}
