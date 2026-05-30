use super::*;

/// Clear `GIT_CONFIG_GLOBAL` from the process environment so the
/// env-gate fallback in `env_satisfies` doesn't interfere.  Requires nextest.
fn clear_git_env() {
    assert!(
        std::env::var("NEXTEST").is_ok(),
        "this test mutates process env and requires nextest (cargo nextest run)"
    );
    unsafe { std::env::remove_var("GIT_CONFIG_GLOBAL") };
}

// Helper to build a ShellSegment for is_likely_successful tests
fn seg(command: &str) -> ShellSegment {
    let words = parse::tokenize(command);
    ShellSegment {
        command: command.to_string(),
        words,
        redirection: None,
        substitutions: vec![],
    }
}

fn seg_with_subst(command: &str) -> ShellSegment {
    let words = parse::tokenize(command);
    ShellSegment {
        command: command.to_string(),
        words,
        redirection: None,
        substitutions: vec![parse::SubstitutionSpan {
            start: 0,
            end: 1,
            pipeline: ParsedPipeline {
                segments: vec![],
                operators: vec![],
                structural_substitutions: vec![],
                has_parse_errors: false,
            },
        }],
    }
}

// ── is_likely_successful ──

#[test]
fn likely_success_export() {
    assert!(is_likely_successful(&seg("export FOO=bar")));
}

#[test]
fn likely_success_export_multiple() {
    assert!(is_likely_successful(&seg("export A=1 B=2")));
}

#[test]
fn likely_success_bare_assignment() {
    assert!(is_likely_successful(&seg("FOO=bar")));
}

#[test]
fn likely_success_true() {
    assert!(is_likely_successful(&seg("true")));
}

#[test]
fn likely_success_echo() {
    assert!(is_likely_successful(&seg("echo hello")));
}

#[test]
fn likely_success_printf() {
    assert!(is_likely_successful(&seg("printf '%s\\n' hello")));
}

#[test]
fn likely_success_export_with_subshell_is_not_likely() {
    // export FOO=$(cmd) — the substitution could fail
    assert!(!is_likely_successful(&seg_with_subst("export FOO=$(cmd)")));
}

#[test]
fn likely_success_echo_with_subshell_is_not_likely() {
    assert!(!is_likely_successful(&seg_with_subst("echo $(cmd)")));
}

#[test]
fn likely_success_bare_assignment_with_subshell_is_not_likely() {
    assert!(!is_likely_successful(&seg_with_subst("FOO=$(cmd)")));
}

#[test]
fn likely_success_unknown_command() {
    assert!(!is_likely_successful(&seg("some_command --flag")));
}

#[test]
fn likely_success_git() {
    assert!(!is_likely_successful(&seg("git push")));
}

#[test]
fn likely_success_rm() {
    assert!(!is_likely_successful(&seg("rm -rf /")));
}

// ── extract_segment_env ──

/// Helper: tokenize a command string into words for extract_segment_env/extract_unset_vars tests.
fn words(cmd: &str) -> Vec<parse::Word> {
    parse::tokenize(cmd)
}

#[test]
fn extract_env_export_single() {
    let vars = extract_segment_env(&words("export FOO=bar"));
    assert_eq!(vars, vec![("FOO".into(), "bar".into())]);
}

#[test]
fn extract_env_export_multiple() {
    let vars = extract_segment_env(&words("export A=1 B=2"));
    assert_eq!(
        vars,
        vec![("A".into(), "1".into()), ("B".into(), "2".into())]
    );
}

#[test]
fn extract_env_export_with_path() {
    let vars = extract_segment_env(&words("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai"));
    assert_eq!(
        vars,
        vec![("GIT_CONFIG_GLOBAL".into(), "~/.gitconfig.ai".into())]
    );
}

#[test]
fn extract_env_bare_assignment() {
    let vars = extract_segment_env(&words("FOO=bar"));
    assert_eq!(vars, vec![("FOO".into(), "bar".into())]);
}

#[test]
fn extract_env_export_no_value() {
    // `export FOO` (no =) should not extract anything
    let vars = extract_segment_env(&words("export FOO"));
    assert!(vars.is_empty());
}

#[test]
fn extract_env_export_flags() {
    let vars = extract_segment_env(&words("export -p"));
    assert!(vars.is_empty());
}

#[test]
fn extract_env_non_export() {
    let vars = extract_segment_env(&words("git push"));
    assert!(vars.is_empty());
}

// ── Compound command env accumulation (end-to-end via registry) ──

/// Build a registry with git config_env gating enabled.
fn registry_with_git_env_gate() -> CommandRegistry {
    let mut config = crate::config::Config::default_config();
    config.git.allowed_with_config = vec!["push".into(), "commit".into(), "add".into()];
    config
        .git
        .config_env
        .insert("GIT_CONFIG_GLOBAL".into(), "~/.gitconfig.ai".into());
    CommandRegistry::from_config(&config)
}

#[test]
fn export_semicolon_git_push_allows() {
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main");
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn export_and_git_push_allows() {
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main");
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn multiple_exports_and_git_push_allows() {
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "export PATH=/usr/bin && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
    );
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn export_or_git_push_does_not_allow() {
    clear_git_env();
    // || means git push runs only if export failed → env not set
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai || git push origin main");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn export_pipe_git_push_does_not_allow() {
    clear_git_env();
    // | means subshell boundary → env doesn't propagate
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai | git push origin main");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn unknown_cmd_breaks_and_chain() {
    // unknown_cmd is not is_likely_successful, so && chain breaks
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && unknown_cmd && git push origin main",
    );
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn semicolon_after_unknown_cmd_resumes_accumulation() {
    // ; resets segment_executes to true, so export after ; is accumulated
    let reg = registry_with_git_env_gate();
    let result = reg
        .evaluate("unknown_cmd ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    // Note: still ASK because unknown_cmd itself is ASK (unrecognized),
    // and strictest-wins. Let's verify the git push part specifically.
}

#[test]
fn semicolon_resumes_accumulation_all_known() {
    // echo is allowed AND likely_successful. After ;, export accumulates.
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "echo starting ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
    );
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn bare_assignment_semicolon_git_push_allows() {
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate("GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main");
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn bare_assignment_and_git_push_allows() {
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate("GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main");
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn wrong_export_value_still_asks() {
    let reg = registry_with_git_env_gate();
    let result =
        reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.wrong && git push origin main");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn export_overridden_by_later_export() {
    let reg = registry_with_git_env_gate();
    // First export sets wrong value, second corrects it
    let result = reg.evaluate(
        "export GIT_CONFIG_GLOBAL=wrong ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
    );
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn or_after_export_clears_accumulated_env() {
    clear_git_env();
    // export A=1 && echo ok || export B=2 && git push
    // The || clears accumulated env (conservative: can't determine which
    // path was taken). git push doesn't see GIT_CONFIG_GLOBAL.
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && echo ok || export OTHER=x && git push origin main",
    );
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn echo_and_export_and_git_push_allows() {
    // echo is likely_successful, export is likely_successful, chain holds
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "echo 'Pushing...' && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
    );
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn realistic_claude_pattern() {
    // The actual pattern Claude generates
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "export PATH=/home/user/.cargo/bin:/usr/bin && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && echo 'Pushing...' && git push -u origin feature-branch",
    );
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn force_push_still_asks_with_export() {
    // Force push flags should escalate even with correct env
    let reg = registry_with_git_env_gate();
    let result =
        reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push --force origin main");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn subshell_in_export_breaks_and_chain() {
    // export FOO=$(cmd) && git push — subshell makes export's success unpredictable,
    // so the && chain can't guarantee the next segment executes.
    let reg = registry_with_git_env_gate();
    let result = reg
        .evaluate("export GIT_CONFIG_GLOBAL=$(cat ~/.gitconfig.ai.path) && git push origin main");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn subshell_in_echo_breaks_and_chain() {
    // echo $(cmd) && export FOO=bar && git push — echo with subshell is not
    // likely successful, breaking the chain for subsequent accumulation.
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "echo $(some_status_cmd) && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
    );
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

// ── unset ──

#[test]
fn unset_removes_accumulated_var() {
    clear_git_env();
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset GIT_CONFIG_GLOBAL ; git push origin main",
    );
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn unset_only_removes_named_var() {
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset OTHER_VAR ; git push origin main",
    );
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

#[test]
fn unset_f_does_not_remove_var() {
    // unset -f removes functions, not variables
    let reg = registry_with_git_env_gate();
    let result = reg.evaluate(
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset -f GIT_CONFIG_GLOBAL ; git push origin main",
    );
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

// ── extract_unset_vars ──

#[test]
fn extract_unset_single() {
    assert_eq!(extract_unset_vars(&words("unset FOO")), vec!["FOO"]);
}

#[test]
fn extract_unset_multiple() {
    assert_eq!(
        extract_unset_vars(&words("unset FOO BAR")),
        vec!["FOO", "BAR"]
    );
}

#[test]
fn extract_unset_with_v_flag() {
    assert_eq!(extract_unset_vars(&words("unset -v FOO")), vec!["FOO"]);
}

#[test]
fn extract_unset_with_f_flag() {
    let w = words("unset -f my_func");
    let result = extract_unset_vars(&w);
    assert!(result.is_empty());
}

#[test]
fn extract_unset_mixed_flags() {
    // -f disables var unset, -v re-enables it
    assert_eq!(
        extract_unset_vars(&words("unset -f my_func -v MY_VAR")),
        vec!["MY_VAR"]
    );
}

#[test]
fn extract_unset_not_unset_cmd() {
    assert!(extract_unset_vars(&words("export FOO=bar")).is_empty());
}

// ── env -i wrapper ──

#[test]
fn env_i_clears_accumulated_env_for_wrapped_cmd() {
    clear_git_env();
    let reg = registry_with_git_env_gate();
    let result =
        reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env -i git push origin main");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn env_dash_clears_accumulated_env_for_wrapped_cmd() {
    clear_git_env();
    let reg = registry_with_git_env_gate();
    let result =
        reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env - git push origin main");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn env_without_i_passes_accumulated_env() {
    let reg = registry_with_git_env_gate();
    let result =
        reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env git push origin main");
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}

// ── Project overlay consent annotation ──

/// Build a registry with a project overlay path set.
fn registry_with_project_overlay() -> CommandRegistry {
    let mut config = crate::config::Config::default_config();
    config.project_overlay_path = Some(std::path::PathBuf::from(
        "/fake/repo/.claude/cc-toolgate.toml",
    ));
    CommandRegistry::from_config(&config)
}

#[test]
fn ask_decision_annotated_with_project_overlay_path() {
    let reg = registry_with_project_overlay();
    // "curl" is in the default ask list — should produce ASK with annotation
    let result = reg.evaluate_single("curl https://example.com");
    assert_eq!(result.decision, Decision::Ask);
    assert!(
        result.reason.contains("project config at"),
        "ASK reason should mention project config; got: {}",
        result.reason
    );
    assert!(
        result
            .reason
            .contains("/fake/repo/.claude/cc-toolgate.toml"),
        "ASK reason should include overlay path; got: {}",
        result.reason
    );
}

#[test]
fn allow_decision_not_annotated_with_project_overlay_path() {
    let reg = registry_with_project_overlay();
    // "ls" is in the default allow list — should NOT have annotation
    let result = reg.evaluate_single("ls -la");
    assert_eq!(result.decision, Decision::Allow);
    assert!(
        !result.reason.contains("project config at"),
        "ALLOW reason should not mention project config; got: {}",
        result.reason
    );
}

#[test]
fn deny_decision_not_annotated_with_project_overlay_path() {
    let reg = registry_with_project_overlay();
    // "shred" is in the default deny list — DENY should NOT have annotation
    let result = reg.evaluate_single("shred /etc/passwd");
    assert_eq!(result.decision, Decision::Deny);
    assert!(
        !result.reason.contains("project config at"),
        "DENY reason should not mention project config; got: {}",
        result.reason
    );
}

#[test]
fn no_annotation_without_project_overlay() {
    // Registry without project overlay — no annotation on ASK
    let config = crate::config::Config::default_config();
    let reg = CommandRegistry::from_config(&config);
    let result = reg.evaluate_single("curl https://example.com");
    assert_eq!(result.decision, Decision::Ask);
    assert!(
        !result.reason.contains("project config at"),
        "without project overlay, reason should not mention project config; got: {}",
        result.reason
    );
}

#[test]
fn compound_ask_decision_annotated_with_project_overlay() {
    let reg = registry_with_project_overlay();
    // Compound command where one segment is ASK
    let result = reg.evaluate("ls -la ; curl https://example.com");
    assert_eq!(result.decision, Decision::Ask);
    assert!(
        result.reason.contains("project config at"),
        "compound ASK reason should mention project config; got: {}",
        result.reason
    );
}

// ── background operator ──

#[test]
fn background_operator_evaluates_both_segments() {
    // Background operator (&) separates two commands — both are evaluated,
    // strictest decision wins (rm -rf / → Ask)
    let config = crate::config::Config::default_config();
    let reg = CommandRegistry::from_config(&config);
    let result = reg.evaluate("echo hello & rm -rf /");
    assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
}

#[test]
fn background_operator_allows_benign_commands() {
    // Both commands are benign — background operator alone doesn't escalate
    let config = crate::config::Config::default_config();
    let reg = CommandRegistry::from_config(&config);
    let result = reg.evaluate("echo hello & ls");
    assert_eq!(
        result.decision,
        Decision::Allow,
        "reason: {}",
        result.reason
    );
}
