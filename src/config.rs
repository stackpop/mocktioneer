use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoggingProvider {
    Fastly,
    Stdout,
}

fn default_logging_provider() -> LoggingProvider { LoggingProvider::Fastly }

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct LoggingConfig {
    #[serde(default = "default_logging_provider")]
    pub provider: LoggingProvider,
    #[validate(length(min = 1))]
    pub endpoint: String,
    pub level: log::LevelFilter,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AppConfig {
    #[validate(nested)]
    pub logging: LoggingConfig,
}

impl AppConfig {
    pub fn from_toml_str(s: &str) -> Result<Self, String> {
        let cfg: AppConfig = toml::from_str(s).map_err(|e| format!("toml parse error: {}", e))?;
        cfg.validate().map_err(|e| e.to_string())?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_parses_valid_levels() {
        let cases = [
            ("off", log::LevelFilter::Off),
            ("error", log::LevelFilter::Error),
            ("warn", log::LevelFilter::Warn),
            ("info", log::LevelFilter::Info),
            ("debug", log::LevelFilter::Debug),
            ("trace", log::LevelFilter::Trace),
        ];
        for (lvl, expected) in cases {
            let toml_str = format!(
                "[logging]\nendpoint = \"endpoint\"\nlevel = \"{}\"\n",
                lvl
            );
            let cfg = AppConfig::from_toml_str(&toml_str).expect("should parse valid config");
            assert_eq!(cfg.logging.endpoint, "endpoint");
            assert_eq!(cfg.logging.level, expected);
            // default provider is fastly
            assert_eq!(cfg.logging.provider, LoggingProvider::Fastly);
        }
    }

    #[test]
    fn app_config_rejects_invalid_level() {
        let toml_str = "[logging]\nendpoint = \"ep\"\nlevel = \"verbose\"\n";
        let err = AppConfig::from_toml_str(toml_str).err().expect("should error");
        assert!(err.contains("toml parse error"), "unexpected error: {}", err);
    }

    #[test]
    fn app_config_accepts_stdout_provider_and_endpoint() {
        let toml_str = "[logging]\nprovider=\"stdout\"\nendpoint=\"ignored\"\nlevel=\"info\"\n";
        let cfg = AppConfig::from_toml_str(toml_str).expect("should parse valid config");
        assert_eq!(cfg.logging.provider, LoggingProvider::Stdout);
        assert_eq!(cfg.logging.level, log::LevelFilter::Info);
    }
}
