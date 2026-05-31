//! Configuration loading and overlay merge logic.
//!
//! cc-toolgate ships with sensible defaults embedded in the binary via
//! `config.default.toml`. Overlays merge on top in this order (later wins):
//!
//! 1. Embedded defaults.
//! 2. User overlay at `~/.config/cc-toolgate/config.toml`.
//! 3. Project overlay at `<git-root>/.claude/cc-toolgate.toml` (if CWD is
//!    inside a git repo). Lets a project permit extra commands without
//!    loosening user-global rules.
//!
//! User overlays use full merge semantics: lists extend (deduplicated),
//! scalars override, `remove_<field>` subtracts, and `replace = true`
//! replaces entirely. Project overlays are restricted to additive
//! operations only — `replace` and `remove_*` fields are stripped for
//! security (a repo should not be able to weaken user-global rules).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Embedded default configuration (compiled into the binary from `config.default.toml`).
const DEFAULT_CONFIG: &str = include_str!("../config.default.toml");

// ── Final (merged) config types ──

/// Top-level configuration, produced by merging embedded defaults with
/// an optional user overlay from `~/.config/cc-toolgate/config.toml`.
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// Global settings (e.g. escalate_deny).
    #[serde(default)]
    pub settings: Settings,
    /// Flat command-to-decision mappings (allow, ask, deny lists).
    #[serde(default)]
    pub commands: Commands,
    /// Wrapper commands that execute their arguments as subcommands.
    #[serde(default)]
    pub wrappers: WrapperConfig,
    /// Git subcommand-aware evaluation rules.
    #[serde(default)]
    pub git: GitConfig,
    /// Cargo subcommand-aware evaluation rules.
    #[serde(default)]
    pub cargo: CargoConfig,
    /// kubectl subcommand-aware evaluation rules.
    #[serde(default)]
    pub kubectl: KubectlConfig,
    /// GitHub CLI (gh) subcommand-aware evaluation rules.
    #[serde(default)]
    pub gh: GhConfig,
    /// Path to the project overlay file, if one was loaded.
    /// Set by [`Config::load()`] when a project-level `.claude/cc-toolgate.toml`
    /// is found. Used to annotate ASK decisions with provenance.
    #[serde(skip)]
    pub project_overlay_path: Option<std::path::PathBuf>,
}

/// Global settings that affect evaluation behavior.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Settings {
    /// When true, DENY decisions are escalated to ASK (the user is prompted
    /// instead of being blocked). Useful for operators who want visibility
    /// without hard blocks.
    #[serde(default)]
    pub escalate_deny: bool,
}

/// Flat command name → decision mappings for simple commands.
///
/// Commands in `allow` run silently, `ask` prompts the user, `deny` blocks outright.
/// Unrecognized commands default to ASK.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Commands {
    /// Commands that run silently (e.g. `ls`, `cat`, `grep`).
    #[serde(default)]
    pub allow: Vec<String>,
    /// Commands that require user confirmation (e.g. `rm`, `curl`, `pip`).
    #[serde(default)]
    pub ask: Vec<String>,
    /// Commands that are blocked outright (e.g. `shred`, `dd`, `mkfs`).
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

/// Git subcommand evaluation rules.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct GitConfig {
    /// Subcommands that are always allowed (e.g. `status`, `log`, `diff`, `branch`).
    #[serde(default)]
    pub read_only: Vec<String>,
    /// Subcommands that are unconditionally allowed (regardless of KB classification).
    /// Unlike `read_only`, these are not expected to be read-only — they're simply
    /// trusted by the user (e.g. `add`).
    #[serde(default)]
    pub allow: Vec<String>,
    /// Subcommands that are allowed only when all `config_env` entries match
    /// (e.g. `push`, `pull` when `GIT_CONFIG_GLOBAL=~/.gitconfig.ai`).
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    /// Environment variable requirements for `allowed_with_config` subcommands.
    /// Each entry maps a var name to its required value. All must match (AND).
    /// Checked in the command's inline env first, then the process environment.
    /// When empty, the env-gating feature is disabled and those commands always ASK.
    #[serde(default)]
    pub config_env: HashMap<String, String>,
    /// Flags that indicate a force-push (e.g. `--force`, `-f`, `--force-with-lease`).
    /// Force-pushes always require confirmation regardless of env-gating.
    #[serde(default)]
    pub force_push_flags: Vec<String>,
}

/// Cargo subcommand evaluation rules.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct CargoConfig {
    /// Subcommands that are always allowed (e.g. `build`, `test`, `check`, `clippy`).
    #[serde(default)]
    pub safe_subcommands: Vec<String>,
    /// Subcommands allowed only when all `config_env` entries match.
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    /// Environment variable requirements for `allowed_with_config` subcommands.
    #[serde(default)]
    pub config_env: HashMap<String, String>,
}

/// kubectl subcommand evaluation rules.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct KubectlConfig {
    /// Read-only subcommands that are always allowed (e.g. `get`, `describe`, `logs`).
    #[serde(default)]
    pub read_only: Vec<String>,
    /// Known mutating subcommands that always require confirmation (e.g. `apply`, `delete`).
    #[serde(default)]
    pub mutating: Vec<String>,
    /// Subcommands allowed only when all `config_env` entries match.
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    /// Environment variable requirements for `allowed_with_config` subcommands.
    #[serde(default)]
    pub config_env: HashMap<String, String>,
}

/// GitHub CLI (gh) subcommand evaluation rules.
///
/// gh uses two-word subcommands (e.g. `pr list`, `issue create`), so
/// both two-word and one-word matches are checked.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct GhConfig {
    /// Read-only subcommands (e.g. `pr list`, `pr view`, `status`, `api`).
    #[serde(default)]
    pub read_only: Vec<String>,
    /// Known mutating subcommands (e.g. `pr create`, `pr merge`, `repo delete`).
    #[serde(default)]
    pub mutating: Vec<String>,
    /// Subcommands allowed only when all `config_env` entries match.
    #[serde(default)]
    pub allowed_with_config: Vec<String>,
    /// Environment variable requirements for `allowed_with_config` subcommands.
    #[serde(default)]
    pub config_env: HashMap<String, String>,
}

// ── Overlay types (user config that merges with defaults) ──
//
// These mirror the public config types but use `Option` for scalars and
// include `replace` flags and `remove_*` lists for the merge system.

/// User-provided configuration overlay, deserialized from `~/.config/cc-toolgate/config.toml`.
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
    allow: Vec<String>,
    #[serde(default)]
    allowed_with_config: Vec<String>,
    config_env: Option<HashMap<String, String>>,
    #[serde(default)]
    force_push_flags: Vec<String>,
    #[serde(default)]
    remove_read_only: Vec<String>,
    #[serde(default)]
    remove_allow: Vec<String>,
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
    config_env: Option<HashMap<String, String>>,
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
    config_env: Option<HashMap<String, String>>,
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
    config_env: Option<HashMap<String, String>>,
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
    /// 3. Merge project overlay from <git-root>/.claude/cc-toolgate.toml
    ///    (if CWD is inside a git repo and the file exists)
    ///
    /// Each overlay merges with what's below it: lists extend, scalars override.
    /// Set `replace = true` in any section to replace its defaults entirely.
    /// Use `remove_<field>` lists to subtract specific items.
    ///
    /// When a project overlay is loaded, `project_overlay_path` is set to the
    /// file that was applied. Callers can inspect this to annotate decisions
    /// with provenance information.
    pub fn load() -> Self {
        let mut config = Self::default_config();
        if let Some(overlay) = Self::load_overlay() {
            config.apply_overlay(overlay);
        }
        if let Some((overlay, path)) = Self::load_project_overlay() {
            config.apply_overlay(overlay);
            config.project_overlay_path = Some(path);
        }
        config
    }

    /// Try to load user overlay from ~/.config/cc-toolgate/config.toml.
    fn load_overlay() -> Option<ConfigOverlay> {
        let home = std::env::var_os("HOME")?;
        let path = std::path::Path::new(&home).join(".config/cc-toolgate/config.toml");
        load_overlay_from_path(&path, "config parse error")
    }

    /// Try to load project overlay from <git-root>/.claude/cc-toolgate.toml.
    /// Returns both the parsed overlay and the path it was loaded from.
    ///
    /// Project overlays may only ADD to allow/ask/deny lists. Any `replace` flags
    /// or `remove_*` lists are stripped and a warning is emitted. This prevents a
    /// malicious project config from removing safety rules set at the user level.
    fn load_project_overlay() -> Option<(ConfigOverlay, std::path::PathBuf)> {
        let cwd = std::env::current_dir().ok()?;
        let git_root = find_git_root(&cwd)?;
        let path = git_root.join(".claude/cc-toolgate.toml");
        let mut overlay = load_overlay_from_path(&path, "project config parse error")?;
        strip_project_overlay_dangerous_fields(&mut overlay, &path);
        Some((overlay, path))
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
        merge_list(&mut self.git.allow, g.allow, &g.remove_allow, g.replace);
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
        if let Some(v) = g.config_env {
            self.git.config_env = v;
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
        if let Some(v) = ca.config_env {
            self.cargo.config_env = v;
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
        if let Some(v) = k.config_env {
            self.kubectl.config_env = v;
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
        if let Some(v) = gh.config_env {
            self.gh.config_env = v;
        }
    }

    /// Apply an overlay from a TOML string. Used for testing.
    #[cfg(test)]
    fn apply_overlay_str(&mut self, toml_str: &str) {
        let overlay: ConfigOverlay = toml::from_str(toml_str).unwrap();
        self.apply_overlay(overlay);
    }
}

/// Strip `replace` flags and `remove_*` lists from a project overlay.
///
/// Project overlays are untrusted: they live in the repo and could be crafted
/// by a malicious project to remove safety rules the user relies on. This
/// function enforces the invariant that project overlays can only ADD entries,
/// never remove or replace them. If any dangerous field was non-empty/true, a
/// warning is printed to stderr.
fn strip_project_overlay_dangerous_fields(overlay: &mut ConfigOverlay, path: &std::path::Path) {
    let mut stripped = false;

    // commands
    if overlay.commands.replace
        || !overlay.commands.remove_allow.is_empty()
        || !overlay.commands.remove_ask.is_empty()
        || !overlay.commands.remove_deny.is_empty()
    {
        stripped = true;
    }
    overlay.commands.replace = false;
    overlay.commands.remove_allow.clear();
    overlay.commands.remove_ask.clear();
    overlay.commands.remove_deny.clear();

    // wrappers
    if overlay.wrappers.replace
        || !overlay.wrappers.remove_allow_floor.is_empty()
        || !overlay.wrappers.remove_ask_floor.is_empty()
    {
        stripped = true;
    }
    overlay.wrappers.replace = false;
    overlay.wrappers.remove_allow_floor.clear();
    overlay.wrappers.remove_ask_floor.clear();

    // git
    if overlay.git.replace
        || !overlay.git.remove_read_only.is_empty()
        || !overlay.git.remove_allow.is_empty()
        || !overlay.git.remove_allowed_with_config.is_empty()
        || !overlay.git.remove_force_push_flags.is_empty()
    {
        stripped = true;
    }
    overlay.git.replace = false;
    overlay.git.remove_read_only.clear();
    overlay.git.remove_allow.clear();
    overlay.git.remove_allowed_with_config.clear();
    overlay.git.remove_force_push_flags.clear();

    // cargo
    if overlay.cargo.replace
        || !overlay.cargo.remove_safe_subcommands.is_empty()
        || !overlay.cargo.remove_allowed_with_config.is_empty()
    {
        stripped = true;
    }
    overlay.cargo.replace = false;
    overlay.cargo.remove_safe_subcommands.clear();
    overlay.cargo.remove_allowed_with_config.clear();

    // kubectl
    if overlay.kubectl.replace
        || !overlay.kubectl.remove_read_only.is_empty()
        || !overlay.kubectl.remove_mutating.is_empty()
        || !overlay.kubectl.remove_allowed_with_config.is_empty()
    {
        stripped = true;
    }
    overlay.kubectl.replace = false;
    overlay.kubectl.remove_read_only.clear();
    overlay.kubectl.remove_mutating.clear();
    overlay.kubectl.remove_allowed_with_config.clear();

    // gh
    if overlay.gh.replace
        || !overlay.gh.remove_read_only.is_empty()
        || !overlay.gh.remove_mutating.is_empty()
        || !overlay.gh.remove_allowed_with_config.is_empty()
    {
        stripped = true;
    }
    overlay.gh.replace = false;
    overlay.gh.remove_read_only.clear();
    overlay.gh.remove_mutating.clear();
    overlay.gh.remove_allowed_with_config.clear();

    if stripped {
        eprintln!(
            "cc-toolgate: project overlay at {} attempted to use replace/remove — stripped for security",
            path.display()
        );
    }
}

/// Read and parse a ConfigOverlay from `path`. Returns `None` if the file
/// can't be read (missing, permission denied, etc.); logs to stderr and
/// returns `None` on parse errors.
fn load_overlay_from_path(path: &std::path::Path, err_label: &str) -> Option<ConfigOverlay> {
    let content = std::fs::read_to_string(path).ok()?;
    match toml::from_str(&content) {
        Ok(overlay) => Some(overlay),
        Err(e) => {
            eprintln!("cc-toolgate: {err_label}: {e}");
            None
        }
    }
}

/// Walk up from `start` looking for a `.git` entry (dir for normal repos,
/// file for worktrees). Returns the containing directory, or `None` if no
/// ancestor contains `.git`.
fn find_git_root(start: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
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
        assert!(config.git.config_env.is_empty());
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
            [git.config_env]
            GIT_CONFIG_GLOBAL = "~/.gitconfig.ai"
        "#,
        );
        assert_eq!(
            config.git.config_env.get("GIT_CONFIG_GLOBAL").unwrap(),
            "~/.gitconfig.ai"
        );
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

    // ── Project overlay discovery ──

    /// Make a scratch dir under std::env::temp_dir() unique to this test run.
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cc-toolgate-test-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn find_git_root_finds_dot_git_in_ancestor() {
        let root = scratch_dir("find-root");
        std::fs::create_dir(root.join(".git")).unwrap();
        let deep = root.join("a/b/c");
        std::fs::create_dir_all(&deep).unwrap();

        assert_eq!(find_git_root(&deep), Some(root.clone()));
        assert_eq!(find_git_root(&root), Some(root.clone()));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn project_overlay_file_parses_and_extends_allow() {
        let root = scratch_dir("project-overlay");
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::create_dir(root.join(".claude")).unwrap();
        std::fs::write(
            root.join(".claude/cc-toolgate.toml"),
            r#"
            [commands]
            allow = ["my-project-tool"]
            "#,
        )
        .unwrap();

        let path = root.join(".claude/cc-toolgate.toml");
        let overlay = load_overlay_from_path(&path, "test").expect("parses");

        let mut config = Config::default_config();
        config.apply_overlay(overlay);
        assert!(
            config
                .commands
                .allow
                .contains(&"my-project-tool".to_string())
        );

        std::fs::remove_dir_all(&root).ok();
    }

    // ── Config::load() integration tests ──
    //
    // These tests change the process CWD and HOME, so they require nextest
    // (which runs each test in its own process) for isolation safety.

    /// Assert we are running under nextest (process-per-test isolation).
    fn require_nextest() {
        assert!(
            std::env::var("NEXTEST").is_ok(),
            "this test mutates process CWD/HOME and requires nextest (cargo nextest run)"
        );
    }

    #[test]
    fn config_load_applies_project_overlay() {
        require_nextest();

        let root = scratch_dir("load-project");
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::create_dir(root.join(".claude")).unwrap();
        std::fs::write(
            root.join(".claude/cc-toolgate.toml"),
            r#"
            [commands]
            allow = ["my-test-script"]
            "#,
        )
        .unwrap();

        // Point HOME to a nonexistent dir so no user overlay interferes.
        let fake_home = root.join("fakehome");
        std::fs::create_dir_all(&fake_home).unwrap();
        unsafe { std::env::set_var("HOME", &fake_home) };

        // Change CWD so Config::load() discovers the project overlay.
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();

        let config = Config::load();

        // Restore CWD before any assertions (in case they panic).
        std::env::set_current_dir(&original_dir).unwrap();

        // Default commands are still present.
        assert!(
            config.commands.allow.contains(&"ls".to_string()),
            "default 'ls' should still be in allow list"
        );
        // Project overlay's addition is present.
        assert!(
            config
                .commands
                .allow
                .contains(&"my-test-script".to_string()),
            "project overlay should have added 'my-test-script' to allow list"
        );
        // project_overlay_path should be set.
        let expected_path = root.join(".claude/cc-toolgate.toml");
        assert_eq!(
            config.project_overlay_path,
            Some(expected_path),
            "project_overlay_path should record the overlay file"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn config_load_project_overlay_replace_is_stripped() {
        require_nextest();

        let root = scratch_dir("load-replace-stripped");
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::create_dir(root.join(".claude")).unwrap();
        // A project overlay with replace = true — the replace flag must be
        // stripped for security. The additive items in the overlay are still
        // applied, but the default allow list is NOT discarded.
        std::fs::write(
            root.join(".claude/cc-toolgate.toml"),
            r#"
            [commands]
            replace = true
            allow = ["only-this"]
            ask = ["only-ask"]
            deny = ["only-deny"]
            "#,
        )
        .unwrap();

        let fake_home = root.join("fakehome");
        std::fs::create_dir_all(&fake_home).unwrap();
        unsafe { std::env::set_var("HOME", &fake_home) };

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();

        let config = Config::load();

        std::env::set_current_dir(&original_dir).unwrap();

        // replace = true is stripped: the default allow list is preserved.
        assert!(
            config.commands.allow.contains(&"ls".to_string()),
            "replace should be stripped; default 'ls' must still be in allow list"
        );
        // The additive items from the overlay are still applied.
        assert!(
            config.commands.allow.contains(&"only-this".to_string()),
            "additive allow items from project overlay should still be applied"
        );
        assert!(
            config.commands.ask.contains(&"only-ask".to_string()),
            "additive ask items from project overlay should still be applied"
        );
        assert!(
            config.commands.deny.contains(&"only-deny".to_string()),
            "additive deny items from project overlay should still be applied"
        );

        // Sections NOT mentioned in the overlay are untouched.
        assert!(
            !config.git.read_only.is_empty(),
            "git read_only should be unaffected by a commands-only overlay"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn config_load_project_overlay_remove_deny_is_stripped() {
        require_nextest();

        let root = scratch_dir("load-remove-deny-stripped");
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::create_dir(root.join(".claude")).unwrap();
        // A project overlay that tries to remove items from deny — must be stripped.
        std::fs::write(
            root.join(".claude/cc-toolgate.toml"),
            r#"
            [commands]
            remove_deny = ["shred"]
            "#,
        )
        .unwrap();

        let fake_home = root.join("fakehome");
        std::fs::create_dir_all(&fake_home).unwrap();
        unsafe { std::env::set_var("HOME", &fake_home) };

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();

        let config = Config::load();

        std::env::set_current_dir(&original_dir).unwrap();

        // remove_deny is stripped: "shred" must still be in deny list.
        assert!(
            config.commands.deny.contains(&"shred".to_string()),
            "remove_deny must be stripped; 'shred' should remain in deny list"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn strip_project_overlay_dangerous_fields_clears_all_sections() {
        let path = std::path::PathBuf::from("/fake/path/.claude/cc-toolgate.toml");
        let mut overlay = ConfigOverlay {
            commands: CommandsOverlay {
                replace: true,
                remove_allow: vec!["cat".into()],
                remove_ask: vec!["rm".into()],
                remove_deny: vec!["shred".into()],
                allow: vec!["my-tool".into()],
                ..Default::default()
            },
            wrappers: WrappersOverlay {
                replace: true,
                remove_allow_floor: vec!["xargs".into()],
                remove_ask_floor: vec!["sudo".into()],
                ..Default::default()
            },
            git: GitOverlay {
                replace: true,
                remove_read_only: vec!["status".into()],
                remove_allow: vec!["add".into()],
                remove_allowed_with_config: vec!["push".into()],
                remove_force_push_flags: vec!["--force".into()],
                read_only: vec!["log".into()],
                ..Default::default()
            },
            cargo: CargoOverlay {
                replace: true,
                remove_safe_subcommands: vec!["build".into()],
                remove_allowed_with_config: vec!["publish".into()],
                ..Default::default()
            },
            kubectl: KubectlOverlay {
                replace: true,
                remove_read_only: vec!["get".into()],
                remove_mutating: vec!["apply".into()],
                remove_allowed_with_config: vec!["exec".into()],
                ..Default::default()
            },
            gh: GhOverlay {
                replace: true,
                remove_read_only: vec!["pr list".into()],
                remove_mutating: vec!["pr merge".into()],
                remove_allowed_with_config: vec!["pr create".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        strip_project_overlay_dangerous_fields(&mut overlay, &path);

        // All dangerous fields cleared.
        assert!(!overlay.commands.replace);
        assert!(overlay.commands.remove_allow.is_empty());
        assert!(overlay.commands.remove_ask.is_empty());
        assert!(overlay.commands.remove_deny.is_empty());

        assert!(!overlay.wrappers.replace);
        assert!(overlay.wrappers.remove_allow_floor.is_empty());
        assert!(overlay.wrappers.remove_ask_floor.is_empty());

        assert!(!overlay.git.replace);
        assert!(overlay.git.remove_read_only.is_empty());
        assert!(overlay.git.remove_allow.is_empty());
        assert!(overlay.git.remove_allowed_with_config.is_empty());
        assert!(overlay.git.remove_force_push_flags.is_empty());

        assert!(!overlay.cargo.replace);
        assert!(overlay.cargo.remove_safe_subcommands.is_empty());
        assert!(overlay.cargo.remove_allowed_with_config.is_empty());

        assert!(!overlay.kubectl.replace);
        assert!(overlay.kubectl.remove_read_only.is_empty());
        assert!(overlay.kubectl.remove_mutating.is_empty());
        assert!(overlay.kubectl.remove_allowed_with_config.is_empty());

        assert!(!overlay.gh.replace);
        assert!(overlay.gh.remove_read_only.is_empty());
        assert!(overlay.gh.remove_mutating.is_empty());
        assert!(overlay.gh.remove_allowed_with_config.is_empty());

        // Additive fields are preserved.
        assert_eq!(overlay.commands.allow, vec!["my-tool"]);
        assert_eq!(overlay.git.read_only, vec!["log"]);
    }

    #[test]
    fn strip_project_overlay_no_op_when_safe() {
        let path = std::path::PathBuf::from("/fake/path/.claude/cc-toolgate.toml");
        let mut overlay = ConfigOverlay {
            commands: CommandsOverlay {
                allow: vec!["my-tool".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        // Should not panic, and should leave additive fields intact.
        strip_project_overlay_dangerous_fields(&mut overlay, &path);
        assert_eq!(overlay.commands.allow, vec!["my-tool"]);
        assert!(!overlay.commands.replace);
    }

    #[test]
    fn config_load_no_project_overlay_path_when_absent() {
        require_nextest();

        let root = scratch_dir("load-no-overlay");
        // No .git, no .claude — Config::load() should not find a project overlay.

        let fake_home = root.join("fakehome");
        std::fs::create_dir_all(&fake_home).unwrap();
        unsafe { std::env::set_var("HOME", &fake_home) };

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();

        let config = Config::load();

        std::env::set_current_dir(&original_dir).unwrap();

        assert!(
            config.project_overlay_path.is_none(),
            "project_overlay_path should be None when no overlay is found"
        );

        std::fs::remove_dir_all(&root).ok();
    }
}
