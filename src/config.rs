use serde::Deserialize;

/// Embedded default configuration.
const DEFAULT_CONFIG: &str = include_str!("../config.default.toml");

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub commands: Commands,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub cargo: CargoConfig,
    #[serde(default)]
    pub kubectl: KubectlConfig,
    #[serde(default)]
    pub gh: GhConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub escalate_deny: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct Commands {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub ask: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GitConfig {
    #[serde(default)]
    pub read_only: Vec<String>,
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    /// Env var that must be set for `allowed_with_config` commands to auto-allow.
    /// When empty, those commands fall through to ASK.
    #[serde(default)]
    pub config_env_var: String,
    #[serde(default)]
    pub force_push_flags: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CargoConfig {
    #[serde(default)]
    pub safe_subcommands: Vec<String>,
    /// Subcommands auto-allowed only when `config_env_var` is present.
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    /// Env var that must be set for `allowed_with_config` commands to auto-allow.
    #[serde(default)]
    pub config_env_var: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct KubectlConfig {
    #[serde(default)]
    pub read_only: Vec<String>,
    #[serde(default)]
    pub mutating: Vec<String>,
    /// Subcommands auto-allowed only when `config_env_var` is present.
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    /// Env var that must be set for `allowed_with_config` commands to auto-allow.
    #[serde(default)]
    pub config_env_var: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct GhConfig {
    #[serde(default)]
    pub read_only: Vec<String>,
    #[serde(default)]
    pub mutating: Vec<String>,
    /// Subcommands auto-allowed only when `config_env_var` is present.
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    /// Env var that must be set for `allowed_with_config` commands to auto-allow.
    #[serde(default)]
    pub config_env_var: String,
}

impl Config {
    /// Load the default embedded configuration.
    pub fn default_config() -> Self {
        toml::from_str(DEFAULT_CONFIG).expect("embedded default config must parse")
    }

    /// Load configuration with resolution order:
    /// 1. User config at ~/.config/cc-toolgate/config.toml (if exists)
    /// 2. Fall back to embedded defaults
    ///
    /// User config REPLACES defaults entirely (no merging).
    pub fn load() -> Self {
        if let Some(user_config) = Self::load_user_config() {
            return user_config;
        }
        Self::default_config()
    }

    /// Try to load user config from ~/.config/cc-toolgate/config.toml.
    fn load_user_config() -> Option<Self> {
        let home = std::env::var_os("HOME")?;
        let path = std::path::Path::new(&home)
            .join(".config/cc-toolgate/config.toml");
        let content = std::fs::read_to_string(path).ok()?;
        match toml::from_str(&content) {
            Ok(config) => Some(config),
            Err(e) => {
                eprintln!("cc-toolgate: config parse error: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let config = Config::default_config();
        assert!(!config.commands.allow.is_empty());
        assert!(!config.commands.ask.is_empty());
        assert!(!config.commands.deny.is_empty());
        assert!(!config.git.read_only.is_empty());
        assert!(!config.cargo.safe_subcommands.is_empty());
        assert!(!config.kubectl.read_only.is_empty());
        assert!(!config.gh.read_only.is_empty());
    }

    #[test]
    fn default_config_has_expected_commands() {
        let config = Config::default_config();
        assert!(config.commands.allow.contains(&"ls".to_string()));
        assert!(config.commands.ask.contains(&"rm".to_string()));
        assert!(config.commands.deny.contains(&"shred".to_string()));
    }

    #[test]
    fn default_escalate_deny_is_false() {
        let config = Config::default_config();
        assert!(!config.settings.escalate_deny);
    }

    #[test]
    fn default_git_env_gate_disabled() {
        let config = Config::default_config();
        assert!(config.git.config_env_var.is_empty());
        assert!(config.git.allowed_with_config.is_empty());
    }

    #[test]
    fn custom_config_parses() {
        let toml = r#"
            [settings]
            escalate_deny = true

            [commands]
            allow = ["ls", "cat"]
            ask = ["rm"]
            deny = ["shred"]
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.settings.escalate_deny);
        assert_eq!(config.commands.allow, vec!["ls", "cat"]);
        assert_eq!(config.commands.ask, vec!["rm"]);
        assert_eq!(config.commands.deny, vec!["shred"]);
        // Sections not in custom config default to empty
        assert!(config.git.read_only.is_empty());
    }
}
