use serde::{Deserialize, Serialize};

/// Embedded default configuration.
const DEFAULT_CONFIG: &str = include_str!("../config.default.toml");

// ── Final (merged) config types ──

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub commands: Commands,
    #[serde(default)]
    pub wrappers: WrapperConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub cargo: CargoConfig,
    #[serde(default)]
    pub kubectl: KubectlConfig,
    #[serde(default)]
    pub gh: GhConfig,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub escalate_deny: bool,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Commands {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub ask: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Commands that execute their arguments as subcommands.
/// The wrapped command is extracted and evaluated; the final decision
/// is max(floor, wrapped_command_decision).
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct WrapperConfig {
    /// Wrappers with Allow floor: wrapper is safe, wrapped command determines disposition.
    /// e.g. xargs, parallel, env, nohup, nice, timeout, time, watch
    #[serde(default)]
    pub allow_floor: Vec<String>,
    /// Wrappers with Ask floor: always at least Ask, wrapped command can escalate to Deny.
    /// e.g. sudo, doas, pkexec
    #[serde(default)]
    pub ask_floor: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
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

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct CargoConfig {
    #[serde(default)]
    pub safe_subcommands: Vec<String>,
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    #[serde(default)]
    pub config_env_var: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct KubectlConfig {
    #[serde(default)]
    pub read_only: Vec<String>,
    #[serde(default)]
    pub mutating: Vec<String>,
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    #[serde(default)]
    pub config_env_var: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct GhConfig {
    #[serde(default)]
    pub read_only: Vec<String>,
    #[serde(default)]
    pub mutating: Vec<String>,
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    #[serde(default)]
    pub config_env_var: String,
}

// ── Overlay types (user config that merges with defaults) ──

#[derive(Debug, Deserialize, Default)]
struct ConfigOverlay {
    #[serde(default)]
    settings: SettingsOverlay,
    #[serde(default)]
    commands: CommandsOverlay,
    #[serde(default)]
    wrappers: WrappersOverlay,
    #[serde(default)]
    git: GitOverlay,
    #[serde(default)]
    cargo: CargoOverlay,
    #[serde(default)]
    kubectl: KubectlOverlay,
    #[serde(default)]
    gh: GhOverlay,
}

#[derive(Debug, Deserialize, Default)]
struct SettingsOverlay {
    escalate_deny: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct WrappersOverlay {
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    allow_floor: Vec<String>,
    #[serde(default)]
    ask_floor: Vec<String>,
    #[serde(default)]
    remove_allow_floor: Vec<String>,
    #[serde(default)]
    remove_ask_floor: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CommandsOverlay {
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    ask: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
    #[serde(default)]
    remove_allow: Vec<String>,
    #[serde(default)]
    remove_ask: Vec<String>,
    #[serde(default)]
    remove_deny: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GitOverlay {
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    read_only: Vec<String>,
    #[serde(default)]
    allowed_with_config: Vec<String>,
    config_env_var: Option<String>,
    #[serde(default)]
    force_push_flags: Vec<String>,
    #[serde(default)]
    remove_read_only: Vec<String>,
    #[serde(default)]
    remove_allowed_with_config: Vec<String>,
    #[serde(default)]
    remove_force_push_flags: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CargoOverlay {
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    safe_subcommands: Vec<String>,
    #[serde(default)]
    allowed_with_config: Vec<String>,
    config_env_var: Option<String>,
    #[serde(default)]
    remove_safe_subcommands: Vec<String>,
    #[serde(default)]
    remove_allowed_with_config: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct KubectlOverlay {
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    read_only: Vec<String>,
    #[serde(default)]
    mutating: Vec<String>,
    #[serde(default)]
    allowed_with_config: Vec<String>,
    config_env_var: Option<String>,
    #[serde(default)]
    remove_read_only: Vec<String>,
    #[serde(default)]
    remove_mutating: Vec<String>,
    #[serde(default)]
    remove_allowed_with_config: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GhOverlay {
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    read_only: Vec<String>,
    #[serde(default)]
    mutating: Vec<String>,
    #[serde(default)]
    allowed_with_config: Vec<String>,
    config_env_var: Option<String>,
    #[serde(default)]
    remove_read_only: Vec<String>,
    #[serde(default)]
    remove_mutating: Vec<String>,
    #[serde(default)]
    remove_allowed_with_config: Vec<String>,
}

// ── Merge logic ──

/// Merge a user list into a default list.
/// In replace mode: user list replaces default entirely.
/// In merge mode: remove items first, then extend with additions (deduped).
fn merge_list(base: &mut Vec<String>, add: Vec<String>, remove: &[String], replace: bool) {
    if replace {
        *base = add;
    } else {
        base.retain(|item| !remove.contains(item));
        for item in add {
            if !base.contains(&item) {
                base.push(item);
            }
        }
    }
}

impl Config {
    /// Load the default embedded configuration.
    pub fn default_config() -> Self {
        toml::from_str(DEFAULT_CONFIG).expect("embedded default config must parse")
    }

    /// Load configuration with resolution order:
    /// 1. Start with embedded defaults
    /// 2. Merge user overlay from ~/.config/cc-toolgate/config.toml (if exists)
    ///
    /// User config merges with defaults: lists extend, scalars override.
    /// Set `replace = true` in any section to replace its defaults entirely.
    /// Use `remove_<field>` lists to subtract specific items from defaults.
    pub fn load() -> Self {
        let mut config = Self::default_config();
        if let Some(overlay) = Self::load_overlay() {
            config.apply_overlay(overlay);
        }
        config
    }

    /// Try to load user overlay from ~/.config/cc-toolgate/config.toml.
    fn load_overlay() -> Option<ConfigOverlay> {
        let home = std::env::var_os("HOME")?;
        let path = std::path::Path::new(&home).join(".config/cc-toolgate/config.toml");
        let content = std::fs::read_to_string(path).ok()?;
        match toml::from_str(&content) {
            Ok(overlay) => Some(overlay),
            Err(e) => {
                eprintln!("cc-toolgate: config parse error: {e}");
                None
            }
        }
    }

    /// Apply an overlay on top of this config (merge semantics).
    fn apply_overlay(&mut self, overlay: ConfigOverlay) {
        // Settings: scalar overrides
        if let Some(v) = overlay.settings.escalate_deny {
            self.settings.escalate_deny = v;
        }

        // Commands
        let c = overlay.commands;
        merge_list(
            &mut self.commands.allow,
            c.allow,
            &c.remove_allow,
            c.replace,
        );
        merge_list(&mut self.commands.ask, c.ask, &c.remove_ask, c.replace);
        merge_list(&mut self.commands.deny, c.deny, &c.remove_deny, c.replace);

        // Wrappers
        let w = overlay.wrappers;
        merge_list(
            &mut self.wrappers.allow_floor,
            w.allow_floor,
            &w.remove_allow_floor,
            w.replace,
        );
        merge_list(
            &mut self.wrappers.ask_floor,
            w.ask_floor,
            &w.remove_ask_floor,
            w.replace,
        );

        // Git
        let g = overlay.git;
        merge_list(
            &mut self.git.read_only,
            g.read_only,
            &g.remove_read_only,
            g.replace,
        );
        merge_list(
            &mut self.git.allowed_with_config,
            g.allowed_with_config,
            &g.remove_allowed_with_config,
            g.replace,
        );
        merge_list(
            &mut self.git.force_push_flags,
            g.force_push_flags,
            &g.remove_force_push_flags,
            g.replace,
        );
        if let Some(v) = g.config_env_var {
            self.git.config_env_var = v;
        }

        // Cargo
        let ca = overlay.cargo;
        merge_list(
            &mut self.cargo.safe_subcommands,
            ca.safe_subcommands,
            &ca.remove_safe_subcommands,
            ca.replace,
        );
        merge_list(
            &mut self.cargo.allowed_with_config,
            ca.allowed_with_config,
            &ca.remove_allowed_with_config,
            ca.replace,
        );
        if let Some(v) = ca.config_env_var {
            self.cargo.config_env_var = v;
        }

        // Kubectl
        let k = overlay.kubectl;
        merge_list(
            &mut self.kubectl.read_only,
            k.read_only,
            &k.remove_read_only,
            k.replace,
        );
        merge_list(
            &mut self.kubectl.mutating,
            k.mutating,
            &k.remove_mutating,
            k.replace,
        );
        merge_list(
            &mut self.kubectl.allowed_with_config,
            k.allowed_with_config,
            &k.remove_allowed_with_config,
            k.replace,
        );
        if let Some(v) = k.config_env_var {
            self.kubectl.config_env_var = v;
        }

        // Gh
        let gh = overlay.gh;
        merge_list(
            &mut self.gh.read_only,
            gh.read_only,
            &gh.remove_read_only,
            gh.replace,
        );
        merge_list(
            &mut self.gh.mutating,
            gh.mutating,
            &gh.remove_mutating,
            gh.replace,
        );
        merge_list(
            &mut self.gh.allowed_with_config,
            gh.allowed_with_config,
            &gh.remove_allowed_with_config,
            gh.replace,
        );
        if let Some(v) = gh.config_env_var {
            self.gh.config_env_var = v;
        }
    }

    /// Apply an overlay from a TOML string. Used for testing.
    #[cfg(test)]
    fn apply_overlay_str(&mut self, toml_str: &str) {
        let overlay: ConfigOverlay = toml::from_str(toml_str).unwrap();
        self.apply_overlay(overlay);
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

    // ── Merge semantics ──

    #[test]
    fn overlay_extends_allow_list() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [commands]
            allow = ["my-tool"]
        "#,
        );
        // Default allow list still present
        assert!(config.commands.allow.contains(&"ls".to_string()));
        // New item added
        assert!(config.commands.allow.contains(&"my-tool".to_string()));
    }

    #[test]
    fn overlay_removes_from_allow_list() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [commands]
            remove_allow = ["cat", "find"]
        "#,
        );
        assert!(!config.commands.allow.contains(&"cat".to_string()));
        assert!(!config.commands.allow.contains(&"find".to_string()));
        // Other items still present
        assert!(config.commands.allow.contains(&"ls".to_string()));
    }

    #[test]
    fn default_wrappers_populated() {
        let config = Config::default_config();
        assert!(config.wrappers.allow_floor.contains(&"xargs".to_string()));
        assert!(config.wrappers.allow_floor.contains(&"env".to_string()));
        assert!(config.wrappers.ask_floor.contains(&"sudo".to_string()));
        assert!(config.wrappers.ask_floor.contains(&"doas".to_string()));
        // These should NOT be in commands.allow/ask anymore
        assert!(!config.commands.allow.contains(&"xargs".to_string()));
        assert!(!config.commands.allow.contains(&"env".to_string()));
        assert!(!config.commands.ask.contains(&"sudo".to_string()));
    }

    #[test]
    fn overlay_removes_from_wrappers() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [wrappers]
            remove_allow_floor = ["xargs"]
        "#,
        );
        assert!(!config.wrappers.allow_floor.contains(&"xargs".to_string()));
        // Others untouched
        assert!(config.wrappers.allow_floor.contains(&"env".to_string()));
    }

    #[test]
    fn overlay_extends_wrappers() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [wrappers]
            allow_floor = ["my-wrapper"]
        "#,
        );
        assert!(
            config
                .wrappers
                .allow_floor
                .contains(&"my-wrapper".to_string())
        );
        assert!(config.wrappers.allow_floor.contains(&"xargs".to_string()));
    }

    #[test]
    fn overlay_replace_commands() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [commands]
            replace = true
            allow = ["ls", "cat"]
            ask = ["rm"]
            deny = ["shred"]
        "#,
        );
        assert_eq!(config.commands.allow, vec!["ls", "cat"]);
        assert_eq!(config.commands.ask, vec!["rm"]);
        assert_eq!(config.commands.deny, vec!["shred"]);
    }

    #[test]
    fn overlay_git_env_gate() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [git]
            allowed_with_config = ["commit", "add", "push"]
            config_env_var = "GIT_CONFIG_GLOBAL"
        "#,
        );
        assert_eq!(config.git.config_env_var, "GIT_CONFIG_GLOBAL");
        assert_eq!(
            config.git.allowed_with_config,
            vec!["commit", "add", "push"]
        );
        // Default read_only still present
        assert!(config.git.read_only.contains(&"status".to_string()));
        assert!(config.git.read_only.contains(&"log".to_string()));
    }

    #[test]
    fn overlay_escalate_deny() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [settings]
            escalate_deny = true
        "#,
        );
        assert!(config.settings.escalate_deny);
    }

    #[test]
    fn overlay_omitted_settings_unchanged() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [commands]
            allow = ["my-tool"]
        "#,
        );
        // Settings not in overlay remain at defaults
        assert!(!config.settings.escalate_deny);
    }

    #[test]
    fn overlay_no_duplicates() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [commands]
            allow = ["ls"]
        "#,
        );
        let count = config.commands.allow.iter().filter(|s| *s == "ls").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn overlay_remove_and_add() {
        let mut config = Config::default_config();
        // Move "eval" from deny to ask
        config.apply_overlay_str(
            r#"
            [commands]
            remove_deny = ["eval"]
            ask = ["eval"]
        "#,
        );
        assert!(!config.commands.deny.contains(&"eval".to_string()));
        assert!(config.commands.ask.contains(&"eval".to_string()));
    }

    #[test]
    fn overlay_replace_git() {
        let mut config = Config::default_config();
        config.apply_overlay_str(
            r#"
            [git]
            replace = true
            read_only = ["status", "log"]
            force_push_flags = ["--force"]
        "#,
        );
        assert_eq!(config.git.read_only, vec!["status", "log"]);
        assert_eq!(config.git.force_push_flags, vec!["--force"]);
        assert!(config.git.allowed_with_config.is_empty());
    }

    #[test]
    fn overlay_unrelated_sections_untouched() {
        let mut config = Config::default_config();
        let original_kubectl_read_only = config.kubectl.read_only.clone();
        config.apply_overlay_str(
            r#"
            [git]
            allowed_with_config = ["push"]
            config_env_var = "GIT_CONFIG_GLOBAL"
        "#,
        );
        assert_eq!(config.kubectl.read_only, original_kubectl_read_only);
    }

    #[test]
    fn empty_overlay_changes_nothing() {
        let original = Config::default_config();
        let mut config = Config::default_config();
        config.apply_overlay_str("");
        assert_eq!(config.commands.allow.len(), original.commands.allow.len());
        assert_eq!(config.git.read_only.len(), original.git.read_only.len());
    }
}
