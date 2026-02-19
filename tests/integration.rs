use cc_toolgate::eval::Decision;

fn decision_for(command: &str) -> Decision {
    cc_toolgate::evaluate(command).decision
}

fn reason_for(command: &str) -> String {
    cc_toolgate::evaluate(command).reason
}

macro_rules! decision_test {
    ($name:ident, $cmd:expr, $decision:ident) => {
        #[test]
        fn $name() {
            assert_eq!(decision_for($cmd), Decision::$decision, "command: {}", $cmd,);
        }
    };
}

// ── ALLOW: Basic read-only commands ──

decision_test!(allow_simple_ls, "ls -la", Allow);
decision_test!(allow_tree, "tree /tmp", Allow);
decision_test!(allow_which, "which cargo", Allow);
decision_test!(allow_eza, "eza --icons --git", Allow);
decision_test!(allow_bat, "bat README.md", Allow);
decision_test!(allow_rg, "rg 'pattern' src/", Allow);
decision_test!(allow_fd, "fd '*.rs' src/", Allow);
decision_test!(allow_dust, "dust /home", Allow);
decision_test!(allow_cat, "cat README.md", Allow);
decision_test!(allow_head, "head -20 src/main.rs", Allow);
decision_test!(allow_tail, "tail -f /var/log/syslog", Allow);
decision_test!(allow_echo, "echo hello world", Allow);
decision_test!(allow_printf, "printf '%s\\n' hello", Allow);
decision_test!(allow_grep, "grep -r 'pattern' src/", Allow);
decision_test!(allow_wc, "wc -l src/main.rs", Allow);
decision_test!(allow_sort, "sort /tmp/data.txt", Allow);
decision_test!(allow_diff, "diff a.txt b.txt", Allow);
decision_test!(allow_find, "find . -name '*.rs'", Allow);
decision_test!(allow_pwd, "pwd", Allow);
decision_test!(allow_env, "env", Allow);
decision_test!(allow_uname, "uname -a", Allow);
decision_test!(allow_id, "id", Allow);
decision_test!(allow_whoami, "whoami", Allow);
decision_test!(allow_stat, "stat /tmp", Allow);
decision_test!(allow_realpath, "realpath ./src", Allow);
decision_test!(allow_date, "date +%Y-%m-%d", Allow);
decision_test!(allow_df, "df -h", Allow);
decision_test!(allow_du, "du -sh .", Allow);
decision_test!(allow_sleep, "sleep 1", Allow);
decision_test!(allow_ps, "ps aux", Allow);
decision_test!(allow_xargs, "xargs echo", Allow);
decision_test!(allow_test_bracket, "test -f /tmp/foo", Allow);
decision_test!(allow_cd, "cd /tmp", Allow);
decision_test!(allow_chdir, "chdir /home/user", Allow);
decision_test!(allow_which_single, "which python", Allow);
decision_test!(allow_which_multiple, "which cargo rustc gcc", Allow);

// ── ALLOW: kubectl read-only ──

decision_test!(allow_kubectl_get, "kubectl get pods", Allow);
decision_test!(allow_kubectl_describe, "kubectl describe svc foo", Allow);
decision_test!(allow_kubectl_logs, "kubectl logs pod/foo", Allow);

// ── ALLOW: git read-only ──

decision_test!(allow_git_status, "git status", Allow);
decision_test!(allow_git_log, "git log --oneline -10", Allow);
decision_test!(allow_git_diff, "git diff HEAD~1", Allow);
decision_test!(allow_git_show, "git show HEAD", Allow);
decision_test!(allow_git_branch, "git branch -a", Allow);
decision_test!(allow_git_blame, "git blame src/main.rs", Allow);
decision_test!(allow_git_stash, "git stash list", Allow);
decision_test!(
    allow_git_c_status,
    "git -C /var/home/user/repo status",
    Allow
);
decision_test!(
    allow_git_c_log,
    "git -C ../other-repo log --oneline -5",
    Allow
);

// ── ALLOW: cargo safe subcommands ──

decision_test!(allow_cargo_build, "cargo build --release", Allow);
decision_test!(allow_cargo_test, "cargo test", Allow);
decision_test!(allow_cargo_check, "cargo check", Allow);
decision_test!(allow_cargo_clippy, "cargo clippy", Allow);
decision_test!(allow_cargo_fmt, "cargo fmt", Allow);
decision_test!(allow_cargo_version, "cargo --version", Allow);
decision_test!(allow_cargo_version_short, "cargo -V", Allow);

// ── ALLOW: gh CLI read-only ──

decision_test!(allow_gh_pr_list, "gh pr list", Allow);
decision_test!(allow_gh_pr_view, "gh pr view 123", Allow);
decision_test!(allow_gh_pr_diff, "gh pr diff 123", Allow);
decision_test!(allow_gh_pr_checks, "gh pr checks 123", Allow);
decision_test!(allow_gh_issue_list, "gh issue list", Allow);
decision_test!(allow_gh_issue_view, "gh issue view 42", Allow);
decision_test!(allow_gh_repo_view, "gh repo view owner/repo", Allow);
decision_test!(allow_gh_run_list, "gh run list", Allow);
decision_test!(allow_gh_status, "gh status", Allow);
decision_test!(allow_gh_search, "gh search repos rust", Allow);
decision_test!(allow_gh_api, "gh api repos/owner/repo/pulls", Allow);
decision_test!(allow_gh_auth_status, "gh auth status", Allow);

// ── ASK: Mutating commands ──

decision_test!(ask_mkdir, "mkdir -p /tmp/new", Ask);
decision_test!(ask_touch, "touch /tmp/newfile", Ask);
decision_test!(ask_mv, "mv old.txt new.txt", Ask);
decision_test!(ask_cp, "cp src.txt dst.txt", Ask);
decision_test!(ask_ln, "ln -s target link", Ask);
decision_test!(ask_chmod, "chmod 755 script.sh", Ask);
decision_test!(ask_tee, "tee /tmp/out.txt", Ask);
decision_test!(ask_curl, "curl https://example.com", Ask);
decision_test!(ask_wget, "wget https://example.com/file", Ask);
decision_test!(ask_pip_install, "pip install requests", Ask);
decision_test!(ask_npm_install, "npm install express", Ask);
decision_test!(ask_python, "python3 script.py", Ask);
decision_test!(ask_make, "make -j4", Ask);
decision_test!(ask_rm, "rm -rf /tmp/junk", Ask);
decision_test!(ask_rmdir, "rmdir /tmp/empty", Ask);
decision_test!(ask_unrecognized, "unknown-command --flag", Ask);

// ── ASK: git mutating ──

decision_test!(ask_git_push, "git push origin main", Ask);
decision_test!(
    ask_git_push_with_env,
    "GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main",
    Ask
);
decision_test!(ask_git_pull, "git pull origin main", Ask);
decision_test!(ask_git_add, "git add .", Ask);
decision_test!(ask_git_commit, "git commit -m 'test'", Ask);
decision_test!(ask_force_push, "git push --force origin main", Ask);
decision_test!(ask_force_push_short_flag, "git push -f origin main", Ask);
decision_test!(
    ask_force_push_with_lease,
    "git push --force-with-lease origin main",
    Ask
);
decision_test!(ask_git_c_push, "git -C /some/repo push origin main", Ask);

// ── ASK: cargo mutating ──

decision_test!(ask_cargo_install, "cargo install ripgrep", Ask);
decision_test!(ask_cargo_publish, "cargo publish", Ask);

// ── ASK: kubectl mutating ──

decision_test!(ask_kubectl_apply, "kubectl apply -f deploy.yaml", Ask);
decision_test!(ask_kubectl_delete, "kubectl delete pod foo", Ask);
decision_test!(
    ask_kubectl_rollout,
    "kubectl rollout restart deploy/foo",
    Ask
);
decision_test!(
    ask_kubectl_scale,
    "kubectl scale --replicas=3 deploy/foo",
    Ask
);

// ── ASK: gh CLI mutating ──

decision_test!(ask_gh_pr_create, "gh pr create --title 'Fix'", Ask);
decision_test!(ask_gh_pr_merge, "gh pr merge 123", Ask);
decision_test!(ask_gh_pr_close, "gh pr close 123", Ask);
decision_test!(ask_gh_pr_comment, "gh pr comment 123 --body 'LGTM'", Ask);
decision_test!(ask_gh_issue_create, "gh issue create --title 'Bug'", Ask);
decision_test!(ask_gh_repo_create, "gh repo create my-repo --public", Ask);
decision_test!(ask_gh_release_create, "gh release create v1.0", Ask);
decision_test!(ask_gh_repo_delete, "gh repo delete my-repo --yes", Ask);

// ── ASK: Privilege escalation ──

decision_test!(ask_sudo, "sudo apt install vim", Ask);
decision_test!(ask_su, "su - root", Ask);
decision_test!(ask_doas, "doas pacman -S vim", Ask);

// ── ASK: Version flags on unrecognized commands ──

decision_test!(ask_unrecognized_version, "rustc --version", Ask);
decision_test!(ask_ambiguous_short_v_flag, "node -V", Ask);

// ── DENY ──

decision_test!(deny_shred, "shred /dev/sda", Deny);
decision_test!(deny_dd, "dd if=/dev/zero of=/dev/sda", Deny);
decision_test!(deny_eval, "eval 'rm -rf /'", Deny);
decision_test!(deny_shutdown, "shutdown -h now", Deny);
decision_test!(deny_reboot, "reboot", Deny);
decision_test!(deny_halt, "halt", Deny);
decision_test!(deny_mkfs_dotted, "mkfs.ext4 /dev/sda1", Deny);

// ── Redirection escalation ──

decision_test!(redir_ls_stdout, "ls -la > /tmp/out.txt", Ask);
decision_test!(redir_ls_append, "ls -la >> /tmp/out.txt", Ask);
decision_test!(redir_eza, "eza --icons > files.txt", Ask);
decision_test!(redir_kubectl_get, "kubectl get pods > pods.txt", Ask);
decision_test!(redir_git_status, "git status > /tmp/s.txt", Ask);
decision_test!(redir_stderr, "bat file 2> /tmp/err", Ask);
decision_test!(redir_combined, "bat file &> /tmp/out", Ask);
decision_test!(redir_cargo_build, "cargo build --release > /tmp/log", Ask);
decision_test!(redir_git_log, "git log > /tmp/log.txt", Ask);
decision_test!(redir_gh_pr_list, "gh pr list > /tmp/prs.txt", Ask);
decision_test!(redir_clobber, "echo hi >| file.txt", Ask);
decision_test!(redir_read_write_asks, "cat <> file.txt", Ask);

// ── /dev/null redirection (non-mutating) ──

decision_test!(allow_ls_devnull, "ls -la > /dev/null", Allow);
decision_test!(allow_ls_devnull_stderr, "ls -la 2> /dev/null", Allow);
decision_test!(allow_ls_devnull_combined, "ls -la &> /dev/null", Allow);
decision_test!(allow_cargo_test_devnull, "cargo test 2> /dev/null", Allow);
decision_test!(allow_git_status_devnull, "git status > /dev/null", Allow);
decision_test!(
    ask_ls_devnull_plus_file,
    "ls -la > /tmp/out 2> /dev/null",
    Ask
);

// ── fd duplication (NOT mutation) ──

decision_test!(fd_dup_2_to_1, "ls -la 2>&1", Allow);
decision_test!(fd_dup_1_to_2, "ls -la 1>&2", Allow);
decision_test!(fd_dup_bare_to_2, "ls -la >&2", Allow);
decision_test!(fd_close_2, "ls -la 2>&-", Allow);
decision_test!(fd_dup_with_real_redir, "ls -la > /tmp/out 2>&1", Ask);
decision_test!(fd_dup_cargo_test, "cargo test 2>&1 | rg FAILED", Allow);
decision_test!(fd_dup_to_custom_fd_asks, "ls -la >&3", Ask);
decision_test!(fd_dup_stderr_to_custom_fd_asks, "ls -la 2>&3", Ask);
decision_test!(fd_dup_to_standard_fd_allows, "ls -la >&2", Allow);

// ── Compound commands ──

decision_test!(chain_allow_and_ask, "ls -la && rm -rf /tmp", Ask);
decision_test!(chain_allow_and_deny, "ls -la && shred foo", Deny);
decision_test!(chain_allow_and_allow, "ls -la ; eza --icons", Allow);
decision_test!(
    chain_kubectl_allow_and_allow,
    "kubectl get pods ; kubectl get svc",
    Allow
);
decision_test!(chain_tree_and_bat, "tree . && bat README.md", Allow);
decision_test!(
    chain_kubectl_allow_and_ask,
    "kubectl get pods && kubectl delete pod foo",
    Ask
);
decision_test!(
    chain_allow_and_deny_dd,
    "ls -la ; dd if=/dev/zero of=disk",
    Deny
);
decision_test!(
    chain_git_log_and_gh_pr_list,
    "git log --oneline -5 && gh pr list",
    Allow
);
decision_test!(chain_cd_and_ls, "cd /tmp && ls -la", Allow);

// ── Pipes ──

decision_test!(pipe_allow_allow, "kubectl get pods | rg running", Allow);
decision_test!(pipe_allow_allow_bat, "ls -la | bat", Allow);
decision_test!(pipe_allow_ask, "eza | unknown-tool", Ask);
decision_test!(
    pipe_cat_grep_wc,
    "cat src/main.rs | grep 'fn ' | wc -l",
    Allow
);
decision_test!(
    pipe_find_xargs_grep,
    "find . -name '*.rs' | xargs grep 'TODO'",
    Allow
);
decision_test!(
    chain_echo_and_cat,
    "echo 'checking...' && cat README.md",
    Allow
);
decision_test!(
    cargo_build_and_test,
    "cargo build --release && cargo test",
    Allow
);
decision_test!(cargo_fmt_and_clippy, "cargo fmt && cargo clippy", Allow);

// ── Command substitution ──

decision_test!(subst_both_allowed, "ls $(which cargo)", Allow);
decision_test!(subst_both_allowed_bat_fd, "bat $(fd '*.rs' src/)", Allow);
decision_test!(subst_inner_ask, "ls $(rm -rf /tmp)", Ask);
decision_test!(subst_inner_deny, "ls $(shred foo)", Deny);
decision_test!(subst_all_allowed, "echo $(cat /etc/passwd)", Allow);
decision_test!(subst_backtick_all_allowed, "echo `whoami`", Allow);
decision_test!(subst_nested_all_allowed, "ls $(cat $(which foo))", Allow);
decision_test!(
    subst_single_quoted_not_expanded,
    "echo '$(rm -rf /)'",
    Allow
);
decision_test!(
    subst_process_subst_no_false_redir,
    "diff <(sort a) <(sort b)",
    Allow
);
decision_test!(
    subst_in_compound_allow,
    "ls $(which cargo) && bat $(fd '*.rs')",
    Allow
);
decision_test!(
    subst_in_compound_deny,
    "ls $(shred foo) && bat README.md",
    Deny
);

// ── Quoting ──

decision_test!(quoted_redirect_single, "echo 'hello > world'", Allow);
decision_test!(quoted_chain_double, "echo \"a && b\"", Allow);

// ── Control flow (for, while, if) ──

decision_test!(
    for_loop_body_ls_allows,
    "for i in *; do ls \"$i\"; done",
    Allow
);
decision_test!(for_loop_body_rm_asks, "for i in *; do rm \"$i\"; done", Ask);
decision_test!(
    for_loop_body_shred_denies,
    "for i in *; do shred \"$i\"; done",
    Deny
);
decision_test!(
    while_loop_body_allows,
    "while true; do echo hello; done",
    Allow
);
decision_test!(if_body_allows, "if true; then ls; fi", Allow);
decision_test!(if_body_rm_asks, "if true; then rm foo; fi", Ask);
decision_test!(
    while_heredoc_pipe_shred_denies,
    "while true; do shred /dev/sda; done <<EOF | cat\nstuff\nEOF",
    Deny
);
decision_test!(
    for_heredoc_pipe_ls_allows,
    "for f in *; do ls \"$f\"; done <<EOF | grep foo\ndata\nEOF",
    Allow
);

// ── Wrapper commands ──

decision_test!(xargs_rm_asks, "xargs rm -rf", Ask);
decision_test!(xargs_shred_denies, "xargs shred", Deny);
decision_test!(xargs_grep_allows, "xargs grep pattern", Allow);
decision_test!(xargs_with_flags_rm, "xargs -0 -I {} rm {}", Ask);
decision_test!(env_rm_asks, "env rm -rf /tmp/test", Ask);
decision_test!(env_with_vars_rm_asks, "env FOO=bar rm -rf /tmp/test", Ask);
decision_test!(env_ls_allows, "env ls -la", Allow);
decision_test!(env_with_vars_ls_allows, "env HOME=/tmp ls -la", Allow);
decision_test!(
    env_kubectl_apply_asks,
    "env KUBECONFIG=~/.kube/config kubectl apply -f foo.yaml",
    Ask
);
decision_test!(sudo_ls_asks, "sudo ls", Ask);
decision_test!(sudo_shred_denies, "sudo shred /dev/sda", Deny);
decision_test!(sudo_rm_asks, "sudo rm -rf /", Ask);
decision_test!(sudo_with_user_flag, "sudo -u postgres psql", Ask);
decision_test!(doas_rm_asks, "doas rm -rf /", Ask);
decision_test!(doas_shred_denies, "doas shred /dev/sda", Deny);
decision_test!(su_rm_asks, "su -c rm", Ask);
decision_test!(nohup_rm_asks, "nohup rm -rf /tmp/test", Ask);
decision_test!(nice_ls_allows, "nice -n 10 ls -la", Allow);
decision_test!(timeout_rm_asks, "timeout 30 rm -rf /tmp/test", Ask);
decision_test!(time_ls_allows, "time ls -la", Allow);
decision_test!(watch_kubectl_get_allows, "watch kubectl get pods", Allow);
decision_test!(watch_rm_asks, "watch rm -rf /tmp/test", Ask);
decision_test!(parallel_rm_asks, "parallel rm", Ask);
decision_test!(parallel_grep_allows, "parallel grep pattern", Allow);
decision_test!(bare_env_allows, "env", Allow);
decision_test!(bare_sudo_asks, "sudo", Ask);
decision_test!(xargs_echo_redir_asks, "xargs echo > /tmp/out", Ask);

// ═══════════════════════════════════════════════════════════════════════════
// Complex tests — reason assertions, custom registries, multi-line heredocs
// ═══════════════════════════════════════════════════════════════════════════

// ── Substitution with reason ──

#[test]
fn subst_double_quoted_expanded() {
    assert_eq!(decision_for("echo \"$(rm -rf /)\""), Decision::Ask);
    let r = reason_for("echo \"$(rm -rf /)\"");
    assert!(
        r.contains("subst"),
        "should show substitution evaluation: {r}"
    );
}

// ── escalate_deny ──

#[test]
fn escalate_deny_turns_deny_to_ask() {
    let config = cc_toolgate::config::Config::default_config();
    let mut registry = cc_toolgate::eval::CommandRegistry::from_config(&config);
    registry.set_escalate_deny(true);
    let result = registry.evaluate("shred /dev/sda");
    assert_eq!(result.decision, Decision::Ask);
    assert!(result.reason.contains("escalated from deny"));
}

#[test]
fn escalate_deny_does_not_affect_allow() {
    let config = cc_toolgate::config::Config::default_config();
    let mut registry = cc_toolgate::eval::CommandRegistry::from_config(&config);
    registry.set_escalate_deny(true);
    let result = registry.evaluate("ls -la");
    assert_eq!(result.decision, Decision::Allow);
}

#[test]
fn escalate_deny_does_not_affect_ask() {
    let config = cc_toolgate::config::Config::default_config();
    let mut registry = cc_toolgate::eval::CommandRegistry::from_config(&config);
    registry.set_escalate_deny(true);
    let result = registry.evaluate("rm -rf /tmp");
    assert_eq!(result.decision, Decision::Ask);
}

#[test]
fn escalate_deny_compound() {
    let config = cc_toolgate::config::Config::default_config();
    let mut registry = cc_toolgate::eval::CommandRegistry::from_config(&config);
    registry.set_escalate_deny(true);
    let result = registry.evaluate("ls -la && shred foo");
    assert_eq!(result.decision, Decision::Ask);
}

// ── Heredoc with markdown (regression: backticks extracted as substitutions) ──

#[test]
fn heredoc_gh_pr_create_with_markdown() {
    let cmd = "gh pr create --title \"Fix\" --body \"$(cat <<'EOF'\n## Summary\n- **New:** `config.rs`\n- **Changed:** `eval/mod.rs`\nEOF\n)\"";
    assert_eq!(decision_for(cmd), Decision::Ask);
    let r = reason_for(cmd);
    assert!(
        r.contains("1 substitution(s)"),
        "should have 1 substitution, not many: {r}"
    );
    assert!(
        !r.contains("unrecognized command"),
        "heredoc body should not produce unrecognized commands: {r}"
    );
}

#[test]
fn heredoc_git_commit_with_body() {
    let cmd = "git commit -m \"$(cat <<'EOF'\nFix bug in `parse/shell.rs`\n\nCo-Authored-By: Claude\nEOF\n)\"";
    assert_eq!(decision_for(cmd), Decision::Ask);
    let r = reason_for(cmd);
    assert!(
        r.contains("1 substitution(s)"),
        "should have 1 substitution, not many: {r}"
    );
    assert!(
        !r.contains("unrecognized command"),
        "heredoc body should not produce unrecognized commands: {r}"
    );
}

// ── Heredoc pipe-swallowing regression tests ──

#[test]
fn heredoc_pipe_kubectl_apply_asks() {
    let cmd = "cat <<'EOF' | kubectl apply -f -\napiVersion: rbac.authorization.k8s.io/v1\nkind: Role\nmetadata:\n  name: ai-agent\n  namespace: external-secrets\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Ask,
        "heredoc piped to kubectl apply MUST require confirmation"
    );
}

#[test]
fn heredoc_pipe_kubectl_delete_asks() {
    let cmd = "cat <<'EOF' | kubectl delete -f -\napiVersion: v1\nkind: Pod\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Ask,
        "heredoc piped to kubectl delete MUST require confirmation"
    );
}

#[test]
fn heredoc_pipe_rm_asks() {
    let cmd = "cat <<'EOF' | xargs rm\nfile1\nfile2\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Ask,
        "heredoc piped to rm MUST require confirmation"
    );
}

#[test]
fn heredoc_pipe_to_grep_allows() {
    let cmd = "cat <<'EOF' | grep pattern\nsome text\npattern here\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Allow,
        "heredoc piped to grep should be allowed"
    );
}

#[test]
fn heredoc_and_dangerous_command_asks() {
    let cmd = "cat <<'EOF' && rm -rf /tmp/test\nbody\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Ask,
        "heredoc && rm should require confirmation"
    );
}

#[test]
fn heredoc_semicolon_dangerous_command_asks() {
    let cmd = "cat <<'EOF' ; kubectl delete pod foo\nbody\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Ask,
        "heredoc ; kubectl delete should require confirmation"
    );
}

#[test]
fn heredoc_or_dangerous_command_asks() {
    let cmd = "cat <<'EOF' || rm -rf /\nbody\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Ask,
        "heredoc || rm should require confirmation"
    );
}

#[test]
fn heredoc_pipe_shred_denies() {
    let cmd = "cat <<'EOF' | xargs shred\nfile1\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Deny,
        "heredoc piped to shred should be denied"
    );
}

#[test]
fn heredoc_pipe_eval_denies() {
    let cmd = "cat <<'EOF' | eval\nmalicious command\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Deny,
        "heredoc piped to eval should be denied"
    );
}

#[test]
fn heredoc_unquoted_pipe_kubectl_asks() {
    let cmd = "cat <<EOF | kubectl apply -f -\napiVersion: v1\nkind: Secret\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Ask,
        "unquoted heredoc piped to kubectl apply MUST require confirmation"
    );
}

#[test]
fn heredoc_pipe_kubectl_reason_mentions_kubectl() {
    let cmd = "cat <<'EOF' | kubectl apply -f -\napiVersion: v1\nEOF\n";
    let r = reason_for(cmd);
    assert!(r.contains("kubectl"), "reason should mention kubectl: {r}");
    assert!(r.contains("|"), "reason should mention pipe operator: {r}");
}

#[test]
fn heredoc_only_no_pipe_cat_allowed() {
    let cmd = "cat <<'EOF'\njust printing text\nEOF\n";
    assert_eq!(
        decision_for(cmd),
        Decision::Allow,
        "cat with heredoc and no pipe should be allowed"
    );
}
