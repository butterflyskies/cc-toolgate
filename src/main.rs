use serde::Deserialize;
use std::io::Read;

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
}

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("failed to read stdin");
        std::process::exit(1);
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("JSON parse error: {e}");
            std::process::exit(1);
        }
    };

    if hook_input.tool_name.as_deref() != Some("Bash") {
        std::process::exit(0);
    }

    let command = hook_input
        .tool_input
        .and_then(|t| t.command)
        .unwrap_or_default();

    if command.is_empty() {
        std::process::exit(0);
    }

    // Init logging (best-effort, no-op on failure)
    cc_toolgate::logging::init();

    let result = cc_toolgate::evaluate(&command);

    // Log decision to ~/.local/share/cc-toolgate/decisions.log
    cc_toolgate::logging::log_decision(&command, &result);

    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": result.decision.as_str(),
            "permissionDecisionReason": result.reason,
        }
    });

    println!("{}", serde_json::to_string(&output).unwrap());
}

#[cfg(test)]
mod tests {
    use cc_toolgate::eval::Decision;

    fn decision_for(command: &str) -> Decision {
        cc_toolgate::evaluate(command).decision
    }

    fn reason_for(command: &str) -> String {
        cc_toolgate::evaluate(command).reason
    }

    // ── ALLOW ──

    #[test]
    fn allow_simple_ls() {
        assert_eq!(decision_for("ls -la"), Decision::Allow);
    }

    #[test]
    fn allow_tree() {
        assert_eq!(decision_for("tree /tmp"), Decision::Allow);
    }

    #[test]
    fn allow_which() {
        assert_eq!(decision_for("which cargo"), Decision::Allow);
    }

    #[test]
    fn allow_eza() {
        assert_eq!(decision_for("eza --icons --git"), Decision::Allow);
    }

    #[test]
    fn allow_bat() {
        assert_eq!(decision_for("bat README.md"), Decision::Allow);
    }

    #[test]
    fn allow_rg() {
        assert_eq!(decision_for("rg 'pattern' src/"), Decision::Allow);
    }

    #[test]
    fn allow_fd() {
        assert_eq!(decision_for("fd '*.rs' src/"), Decision::Allow);
    }

    #[test]
    fn allow_dust() {
        assert_eq!(decision_for("dust /home"), Decision::Allow);
    }

    #[test]
    fn allow_kubectl_get() {
        assert_eq!(decision_for("kubectl get pods"), Decision::Allow);
    }

    #[test]
    fn allow_kubectl_describe() {
        assert_eq!(decision_for("kubectl describe svc foo"), Decision::Allow);
    }

    #[test]
    fn allow_kubectl_logs() {
        assert_eq!(decision_for("kubectl logs pod/foo"), Decision::Allow);
    }

    #[test]
    fn allow_git_push_with_config() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main"),
            Decision::Allow
        );
    }

    #[test]
    fn allow_git_pull_with_config() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git pull origin main"),
            Decision::Allow
        );
    }

    #[test]
    fn allow_git_status_with_config() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git status"),
            Decision::Allow
        );
    }

    #[test]
    fn allow_git_add_with_config() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git add ."),
            Decision::Allow
        );
    }

    #[test]
    fn allow_cargo_build() {
        assert_eq!(decision_for("cargo build --release"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_test() {
        assert_eq!(decision_for("cargo test"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_check() {
        assert_eq!(decision_for("cargo check"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_clippy() {
        assert_eq!(decision_for("cargo clippy"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_fmt() {
        assert_eq!(decision_for("cargo fmt"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_version() {
        assert_eq!(decision_for("cargo --version"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_version_short() {
        assert_eq!(decision_for("cargo -V"), Decision::Allow);
    }

    #[test]
    fn allow_generic_version() {
        assert_eq!(decision_for("rustc --version"), Decision::Allow);
    }

    #[test]
    fn ask_ambiguous_short_v_flag() {
        // -V is NOT universally --version (e.g. tar -V = --verbose)
        // node is in ASK_COMMANDS, and -V alone doesn't auto-allow
        assert_eq!(decision_for("node -V"), Decision::Ask);
    }

    // ── New ALLOW: read-only commands ──

    #[test]
    fn allow_cat() {
        assert_eq!(decision_for("cat README.md"), Decision::Allow);
    }

    #[test]
    fn allow_head() {
        assert_eq!(decision_for("head -20 src/main.rs"), Decision::Allow);
    }

    #[test]
    fn allow_tail() {
        assert_eq!(decision_for("tail -f /var/log/syslog"), Decision::Allow);
    }

    #[test]
    fn allow_echo() {
        assert_eq!(decision_for("echo hello world"), Decision::Allow);
    }

    #[test]
    fn allow_printf() {
        assert_eq!(decision_for("printf '%s\\n' hello"), Decision::Allow);
    }

    #[test]
    fn allow_grep() {
        assert_eq!(decision_for("grep -r 'pattern' src/"), Decision::Allow);
    }

    #[test]
    fn allow_wc() {
        assert_eq!(decision_for("wc -l src/main.rs"), Decision::Allow);
    }

    #[test]
    fn allow_sort() {
        assert_eq!(decision_for("sort /tmp/data.txt"), Decision::Allow);
    }

    #[test]
    fn allow_diff() {
        assert_eq!(decision_for("diff a.txt b.txt"), Decision::Allow);
    }

    #[test]
    fn allow_find() {
        assert_eq!(decision_for("find . -name '*.rs'"), Decision::Allow);
    }

    #[test]
    fn allow_pwd() {
        assert_eq!(decision_for("pwd"), Decision::Allow);
    }

    #[test]
    fn allow_env() {
        assert_eq!(decision_for("env"), Decision::Allow);
    }

    #[test]
    fn allow_uname() {
        assert_eq!(decision_for("uname -a"), Decision::Allow);
    }

    #[test]
    fn allow_id() {
        assert_eq!(decision_for("id"), Decision::Allow);
    }

    #[test]
    fn allow_whoami() {
        assert_eq!(decision_for("whoami"), Decision::Allow);
    }

    #[test]
    fn allow_stat() {
        assert_eq!(decision_for("stat /tmp"), Decision::Allow);
    }

    #[test]
    fn allow_realpath() {
        assert_eq!(decision_for("realpath ./src"), Decision::Allow);
    }

    #[test]
    fn allow_date() {
        assert_eq!(decision_for("date +%Y-%m-%d"), Decision::Allow);
    }

    #[test]
    fn allow_df() {
        assert_eq!(decision_for("df -h"), Decision::Allow);
    }

    #[test]
    fn allow_du() {
        assert_eq!(decision_for("du -sh ."), Decision::Allow);
    }

    #[test]
    fn allow_sleep() {
        assert_eq!(decision_for("sleep 1"), Decision::Allow);
    }

    #[test]
    fn allow_ps() {
        assert_eq!(decision_for("ps aux"), Decision::Allow);
    }

    #[test]
    fn allow_xargs() {
        assert_eq!(decision_for("xargs echo"), Decision::Allow);
    }

    #[test]
    fn allow_test_bracket() {
        assert_eq!(decision_for("test -f /tmp/foo"), Decision::Allow);
    }

    // ── New ASK: mutating but needed ──

    #[test]
    fn ask_mkdir() {
        assert_eq!(decision_for("mkdir -p /tmp/new"), Decision::Ask);
    }

    #[test]
    fn ask_touch() {
        assert_eq!(decision_for("touch /tmp/newfile"), Decision::Ask);
    }

    #[test]
    fn ask_mv() {
        assert_eq!(decision_for("mv old.txt new.txt"), Decision::Ask);
    }

    #[test]
    fn ask_cp() {
        assert_eq!(decision_for("cp src.txt dst.txt"), Decision::Ask);
    }

    #[test]
    fn ask_ln() {
        assert_eq!(decision_for("ln -s target link"), Decision::Ask);
    }

    #[test]
    fn ask_chmod() {
        assert_eq!(decision_for("chmod 755 script.sh"), Decision::Ask);
    }

    #[test]
    fn ask_tee() {
        assert_eq!(decision_for("tee /tmp/out.txt"), Decision::Ask);
    }

    #[test]
    fn ask_curl() {
        assert_eq!(decision_for("curl https://example.com"), Decision::Ask);
    }

    #[test]
    fn ask_wget() {
        assert_eq!(decision_for("wget https://example.com/file"), Decision::Ask);
    }

    #[test]
    fn ask_pip_install() {
        assert_eq!(decision_for("pip install requests"), Decision::Ask);
    }

    #[test]
    fn ask_npm_install() {
        assert_eq!(decision_for("npm install express"), Decision::Ask);
    }

    #[test]
    fn ask_python() {
        assert_eq!(decision_for("python3 script.py"), Decision::Ask);
    }

    #[test]
    fn ask_make() {
        assert_eq!(decision_for("make -j4"), Decision::Ask);
    }

    // ── Compound: new allow commands in pipelines ──

    #[test]
    fn pipe_cat_grep_wc() {
        assert_eq!(
            decision_for("cat src/main.rs | grep 'fn ' | wc -l"),
            Decision::Allow
        );
    }

    #[test]
    fn pipe_find_xargs_grep() {
        assert_eq!(
            decision_for("find . -name '*.rs' | xargs grep 'TODO'"),
            Decision::Allow
        );
    }

    #[test]
    fn chain_echo_and_cat() {
        assert_eq!(
            decision_for("echo 'checking...' && cat README.md"),
            Decision::Allow
        );
    }

    // ── ASK ──

    #[test]
    fn ask_rm() {
        assert_eq!(decision_for("rm -rf /tmp/junk"), Decision::Ask);
    }

    #[test]
    fn ask_rmdir() {
        assert_eq!(decision_for("rmdir /tmp/empty"), Decision::Ask);
    }

    #[test]
    fn ask_sudo() {
        assert_eq!(decision_for("sudo apt install vim"), Decision::Ask);
    }

    #[test]
    fn ask_su() {
        assert_eq!(decision_for("su - root"), Decision::Ask);
    }

    #[test]
    fn ask_doas() {
        assert_eq!(decision_for("doas pacman -S vim"), Decision::Ask);
    }

    #[test]
    fn ask_git_push_no_config() {
        assert_eq!(decision_for("git push origin main"), Decision::Ask);
    }

    #[test]
    fn ask_git_pull_no_config() {
        assert_eq!(decision_for("git pull origin main"), Decision::Ask);
    }

    #[test]
    fn ask_force_push() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push --force origin main"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_force_push_short_flag() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push -f origin main"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_force_push_with_lease() {
        assert_eq!(
            decision_for(
                "GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push --force-with-lease origin main"
            ),
            Decision::Ask
        );
    }

    #[test]
    fn ask_git_commit() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git commit -m 'test'"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_kubectl_apply() {
        assert_eq!(decision_for("kubectl apply -f deploy.yaml"), Decision::Ask);
    }

    #[test]
    fn ask_kubectl_delete() {
        assert_eq!(decision_for("kubectl delete pod foo"), Decision::Ask);
    }

    #[test]
    fn ask_kubectl_rollout() {
        assert_eq!(
            decision_for("kubectl rollout restart deploy/foo"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_kubectl_scale() {
        assert_eq!(
            decision_for("kubectl scale --replicas=3 deploy/foo"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_unrecognized() {
        assert_eq!(decision_for("unknown-command --flag"), Decision::Ask);
    }

    #[test]
    fn ask_cargo_install() {
        assert_eq!(decision_for("cargo install ripgrep"), Decision::Ask);
    }

    #[test]
    fn ask_cargo_publish() {
        assert_eq!(decision_for("cargo publish"), Decision::Ask);
    }

    // ── Redirection escalation ──

    #[test]
    fn redir_ls_stdout() {
        assert_eq!(decision_for("ls -la > /tmp/out.txt"), Decision::Ask);
    }

    #[test]
    fn redir_ls_append() {
        assert_eq!(decision_for("ls -la >> /tmp/out.txt"), Decision::Ask);
    }

    #[test]
    fn redir_eza() {
        assert_eq!(decision_for("eza --icons > files.txt"), Decision::Ask);
    }

    #[test]
    fn redir_kubectl_get() {
        assert_eq!(decision_for("kubectl get pods > pods.txt"), Decision::Ask);
    }

    #[test]
    fn redir_git_status() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git status > /tmp/s.txt"),
            Decision::Ask
        );
    }

    #[test]
    fn redir_stderr() {
        assert_eq!(decision_for("bat file 2> /tmp/err"), Decision::Ask);
    }

    #[test]
    fn redir_combined() {
        assert_eq!(decision_for("bat file &> /tmp/out"), Decision::Ask);
    }

    #[test]
    fn redir_cargo_build() {
        assert_eq!(
            decision_for("cargo build --release > /tmp/log"),
            Decision::Ask
        );
    }

    // ── fd duplication (NOT mutation) ──

    #[test]
    fn fd_dup_2_to_1() {
        assert_eq!(decision_for("ls -la 2>&1"), Decision::Allow);
    }

    #[test]
    fn fd_dup_1_to_2() {
        assert_eq!(decision_for("ls -la 1>&2"), Decision::Allow);
    }

    #[test]
    fn fd_dup_bare_to_2() {
        assert_eq!(decision_for("ls -la >&2"), Decision::Allow);
    }

    #[test]
    fn fd_close_2() {
        assert_eq!(decision_for("ls -la 2>&-"), Decision::Allow);
    }

    #[test]
    fn fd_dup_with_real_redir() {
        assert_eq!(decision_for("ls -la > /tmp/out 2>&1"), Decision::Ask);
    }

    #[test]
    fn fd_dup_cargo_test() {
        assert_eq!(
            decision_for("cargo test 2>&1 | rg FAILED"),
            Decision::Allow
        );
    }

    // ── DENY ──

    #[test]
    fn deny_shred() {
        assert_eq!(decision_for("shred /dev/sda"), Decision::Deny);
    }

    #[test]
    fn deny_dd() {
        assert_eq!(decision_for("dd if=/dev/zero of=/dev/sda"), Decision::Deny);
    }

    #[test]
    fn deny_eval() {
        assert_eq!(decision_for("eval 'rm -rf /'"), Decision::Deny);
    }

    #[test]
    fn deny_shutdown() {
        assert_eq!(decision_for("shutdown -h now"), Decision::Deny);
    }

    #[test]
    fn deny_reboot() {
        assert_eq!(decision_for("reboot"), Decision::Deny);
    }

    #[test]
    fn deny_halt() {
        assert_eq!(decision_for("halt"), Decision::Deny);
    }

    #[test]
    fn deny_mkfs_dotted() {
        assert_eq!(decision_for("mkfs.ext4 /dev/sda1"), Decision::Deny);
    }

    // ── Compound commands ──

    #[test]
    fn chain_allow_and_ask() {
        assert_eq!(decision_for("ls -la && rm -rf /tmp"), Decision::Ask);
    }

    #[test]
    fn chain_allow_and_deny() {
        assert_eq!(decision_for("ls -la && shred foo"), Decision::Deny);
    }

    #[test]
    fn chain_allow_and_allow() {
        assert_eq!(decision_for("ls -la ; eza --icons"), Decision::Allow);
    }

    #[test]
    fn chain_kubectl_allow_and_allow() {
        assert_eq!(
            decision_for("kubectl get pods ; kubectl get svc"),
            Decision::Allow
        );
    }

    #[test]
    fn chain_tree_and_bat() {
        assert_eq!(decision_for("tree . && bat README.md"), Decision::Allow);
    }

    #[test]
    fn chain_kubectl_allow_and_ask() {
        assert_eq!(
            decision_for("kubectl get pods && kubectl delete pod foo"),
            Decision::Ask
        );
    }

    #[test]
    fn chain_allow_and_deny_dd() {
        assert_eq!(
            decision_for("ls -la ; dd if=/dev/zero of=disk"),
            Decision::Deny
        );
    }

    // ── Pipes ──

    #[test]
    fn pipe_allow_allow() {
        assert_eq!(
            decision_for("kubectl get pods | rg running"),
            Decision::Allow
        );
    }

    #[test]
    fn pipe_allow_allow_bat() {
        assert_eq!(decision_for("ls -la | bat"), Decision::Allow);
    }

    #[test]
    fn pipe_allow_ask() {
        assert_eq!(decision_for("eza | unknown-tool"), Decision::Ask);
    }

    // ── Substitution (recursive evaluation) ──

    #[test]
    fn subst_both_allowed() {
        assert_eq!(decision_for("ls $(which cargo)"), Decision::Allow);
    }

    #[test]
    fn subst_both_allowed_bat_fd() {
        assert_eq!(decision_for("bat $(fd '*.rs' src/)"), Decision::Allow);
    }

    #[test]
    fn subst_inner_ask() {
        assert_eq!(decision_for("ls $(rm -rf /tmp)"), Decision::Ask);
    }

    #[test]
    fn subst_inner_deny() {
        assert_eq!(decision_for("ls $(shred foo)"), Decision::Deny);
    }

    #[test]
    fn subst_all_allowed() {
        assert_eq!(decision_for("echo $(cat /etc/passwd)"), Decision::Allow);
    }

    #[test]
    fn subst_backtick_all_allowed() {
        assert_eq!(decision_for("echo `whoami`"), Decision::Allow);
    }

    #[test]
    fn subst_nested_all_allowed() {
        assert_eq!(decision_for("ls $(cat $(which foo))"), Decision::Allow);
    }

    #[test]
    fn subst_single_quoted_not_expanded() {
        assert_eq!(decision_for("echo '$(rm -rf /)'"), Decision::Allow);
    }

    #[test]
    fn subst_double_quoted_expanded() {
        assert_eq!(decision_for("echo \"$(rm -rf /)\""), Decision::Ask);
        let r = reason_for("echo \"$(rm -rf /)\"");
        assert!(
            r.contains("subst"),
            "should show substitution evaluation: {r}"
        );
    }

    #[test]
    fn subst_process_subst_no_false_redir() {
        assert_eq!(decision_for("diff <(sort a) <(sort b)"), Decision::Allow);
    }

    #[test]
    fn subst_in_compound_allow() {
        assert_eq!(
            decision_for("ls $(which cargo) && bat $(fd '*.rs')"),
            Decision::Allow
        );
    }

    #[test]
    fn subst_in_compound_deny() {
        assert_eq!(
            decision_for("ls $(shred foo) && bat README.md"),
            Decision::Deny
        );
    }

    // ── Quoting ──

    #[test]
    fn quoted_redirect_single() {
        assert_eq!(decision_for("echo 'hello > world'"), Decision::Allow);
    }

    #[test]
    fn quoted_chain_double() {
        assert_eq!(decision_for("echo \"a && b\""), Decision::Allow);
    }

    // ── Cargo compound ──

    #[test]
    fn cargo_build_and_test() {
        assert_eq!(
            decision_for("cargo build --release && cargo test"),
            Decision::Allow
        );
    }

    #[test]
    fn cargo_fmt_and_clippy() {
        assert_eq!(
            decision_for("cargo fmt && cargo clippy"),
            Decision::Allow
        );
    }

    // ── which ──

    #[test]
    fn allow_which_single() {
        assert_eq!(decision_for("which python"), Decision::Allow);
    }

    #[test]
    fn allow_which_multiple() {
        assert_eq!(decision_for("which cargo rustc gcc"), Decision::Allow);
    }

    // ── cd / chdir ──

    #[test]
    fn allow_cd() {
        assert_eq!(decision_for("cd /tmp"), Decision::Allow);
    }

    #[test]
    fn allow_chdir() {
        assert_eq!(decision_for("chdir /home/user"), Decision::Allow);
    }

    // ── Git read-only (no config required) ──

    #[test]
    fn allow_git_log() {
        assert_eq!(decision_for("git log --oneline -10"), Decision::Allow);
    }

    #[test]
    fn allow_git_diff() {
        assert_eq!(decision_for("git diff HEAD~1"), Decision::Allow);
    }

    #[test]
    fn allow_git_show() {
        assert_eq!(decision_for("git show HEAD"), Decision::Allow);
    }

    #[test]
    fn allow_git_branch() {
        assert_eq!(decision_for("git branch -a"), Decision::Allow);
    }

    #[test]
    fn allow_git_blame() {
        assert_eq!(decision_for("git blame src/main.rs"), Decision::Allow);
    }

    #[test]
    fn allow_git_stash() {
        assert_eq!(decision_for("git stash list"), Decision::Allow);
    }

    #[test]
    fn redir_git_log() {
        assert_eq!(decision_for("git log > /tmp/log.txt"), Decision::Ask);
    }

    // ── gh CLI read-only ──

    #[test]
    fn allow_gh_pr_list() {
        assert_eq!(decision_for("gh pr list"), Decision::Allow);
    }

    #[test]
    fn allow_gh_pr_view() {
        assert_eq!(decision_for("gh pr view 123"), Decision::Allow);
    }

    #[test]
    fn allow_gh_pr_diff() {
        assert_eq!(decision_for("gh pr diff 123"), Decision::Allow);
    }

    #[test]
    fn allow_gh_pr_checks() {
        assert_eq!(decision_for("gh pr checks 123"), Decision::Allow);
    }

    #[test]
    fn allow_gh_issue_list() {
        assert_eq!(decision_for("gh issue list"), Decision::Allow);
    }

    #[test]
    fn allow_gh_issue_view() {
        assert_eq!(decision_for("gh issue view 42"), Decision::Allow);
    }

    #[test]
    fn allow_gh_repo_view() {
        assert_eq!(decision_for("gh repo view owner/repo"), Decision::Allow);
    }

    #[test]
    fn allow_gh_run_list() {
        assert_eq!(decision_for("gh run list"), Decision::Allow);
    }

    #[test]
    fn allow_gh_status() {
        assert_eq!(decision_for("gh status"), Decision::Allow);
    }

    #[test]
    fn allow_gh_search() {
        assert_eq!(decision_for("gh search repos rust"), Decision::Allow);
    }

    #[test]
    fn allow_gh_api() {
        assert_eq!(decision_for("gh api repos/owner/repo/pulls"), Decision::Allow);
    }

    #[test]
    fn allow_gh_auth_status() {
        assert_eq!(decision_for("gh auth status"), Decision::Allow);
    }

    // ── gh CLI mutating ──

    #[test]
    fn ask_gh_pr_create() {
        assert_eq!(decision_for("gh pr create --title 'Fix'"), Decision::Ask);
    }

    #[test]
    fn ask_gh_pr_merge() {
        assert_eq!(decision_for("gh pr merge 123"), Decision::Ask);
    }

    #[test]
    fn ask_gh_pr_close() {
        assert_eq!(decision_for("gh pr close 123"), Decision::Ask);
    }

    #[test]
    fn ask_gh_pr_comment() {
        assert_eq!(decision_for("gh pr comment 123 --body 'LGTM'"), Decision::Ask);
    }

    #[test]
    fn ask_gh_issue_create() {
        assert_eq!(decision_for("gh issue create --title 'Bug'"), Decision::Ask);
    }

    #[test]
    fn ask_gh_repo_create() {
        assert_eq!(decision_for("gh repo create my-repo --public"), Decision::Ask);
    }

    #[test]
    fn ask_gh_release_create() {
        assert_eq!(decision_for("gh release create v1.0"), Decision::Ask);
    }

    #[test]
    fn ask_gh_repo_delete() {
        assert_eq!(decision_for("gh repo delete my-repo --yes"), Decision::Ask);
    }

    // ── gh CLI redirection escalation ──

    #[test]
    fn redir_gh_pr_list() {
        assert_eq!(decision_for("gh pr list > /tmp/prs.txt"), Decision::Ask);
    }

    // ── Compound with new rules ──

    #[test]
    fn chain_git_log_and_gh_pr_list() {
        assert_eq!(
            decision_for("git log --oneline -5 && gh pr list"),
            Decision::Allow
        );
    }

    #[test]
    fn chain_cd_and_ls() {
        assert_eq!(decision_for("cd /tmp && ls -la"), Decision::Allow);
    }
}
