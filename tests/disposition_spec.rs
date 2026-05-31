//! Disposition spec: table-driven tests extracted from the existing test suite.
//!
//! Every (command, expected_decision) pair in the codebase is captured here.
//! This file documents the contract — what the evaluation engine promises
//! for each command string.
//!
//! Sources:
//!   - tests/integration.rs          (212 macro tests + 24 hand-written)
//!   - src/commands/tools/git.rs     (18 unit tests, 12 are disposition tests)
//!   - src/commands/tools/cargo.rs   (13 unit tests, 13 are disposition tests)
//!   - src/commands/tools/gh.rs      (14 unit tests, 14 are disposition tests)
//!   - src/commands/tools/kubectl.rs (11 unit tests, 11 are disposition tests)
//!   - src/commands/simple.rs        (4 unit tests, 4 are disposition tests)
//!   - src/eval/mod.rs               (49 unit tests, 33 are disposition tests)
//!
//! Tests that only assert on internal helpers (is_likely_successful,
//! extract_segment_env, extract_unset_vars, env_satisfies) are NOT included
//! here — they test parser/context internals, not command disposition.

use Decision::*;
use cc_toolgate::eval::Decision;

// ═══════════════════════════════════════════════════════════════════════════════
// Helper: evaluate with default config
// ═══════════════════════════════════════════════════════════════════════════════

fn decision_for(command: &str) -> Decision {
    cc_toolgate::evaluate(command).decision
}

fn reason_for(command: &str) -> String {
    cc_toolgate::evaluate(command).reason
}

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 1: Default config — simple (command, decision) pairs
// ═══════════════════════════════════════════════════════════════════════════════
//
// These use `cc_toolgate::evaluate()` which builds a registry from the embedded
// default config (no user overlay, no config_env).

/// Basic read-only commands that should always be allowed.
/// Source: tests/integration.rs — ALLOW: Basic read-only commands
const ALLOW_READONLY: &[(&str, Decision)] = &[
    ("ls -la", Allow),
    ("tree /tmp", Allow),
    ("which cargo", Allow),
    ("eza --icons --git", Allow),
    ("bat README.md", Allow),
    ("rg 'pattern' src/", Allow),
    ("fd '*.rs' src/", Allow),
    ("dust /home", Allow),
    ("cat README.md", Allow),
    ("head -20 src/main.rs", Allow),
    ("tail -f /var/log/syslog", Allow),
    ("echo hello world", Allow),
    ("printf '%s\\n' hello", Allow),
    ("grep -r 'pattern' src/", Allow),
    ("wc -l src/main.rs", Allow),
    ("sort /tmp/data.txt", Allow),
    ("diff a.txt b.txt", Allow),
    ("find . -name '*.rs'", Allow),
    ("pwd", Allow),
    ("env", Allow),
    ("uname -a", Allow),
    ("id", Allow),
    ("whoami", Allow),
    ("stat /tmp", Allow),
    ("realpath ./src", Allow),
    ("date +%Y-%m-%d", Allow),
    ("df -h", Allow),
    ("du -sh .", Allow),
    ("sleep 1", Allow),
    ("ps aux", Allow),
    ("xargs echo", Allow),
    ("test -f /tmp/foo", Allow),
    ("cd /tmp", Allow),
    ("chdir /home/user", Allow),
    ("which python", Allow),
    ("which cargo rustc gcc", Allow),
    ("source ~/.bashrc", Allow),
    (". /etc/profile", Allow),
];

/// kubectl read-only subcommands.
/// Source: tests/integration.rs — ALLOW: kubectl read-only
const ALLOW_KUBECTL_READONLY: &[(&str, Decision)] = &[
    ("kubectl get pods", Allow),
    ("kubectl describe svc foo", Allow),
    ("kubectl logs pod/foo", Allow),
];

/// Git read-only subcommands.
/// Source: tests/integration.rs — ALLOW: git read-only
const ALLOW_GIT_READONLY: &[(&str, Decision)] = &[
    ("git status", Allow),
    ("git log --oneline -10", Allow),
    ("git diff HEAD~1", Allow),
    ("git show HEAD", Allow),
    ("git branch -a", Allow),
    ("git blame src/main.rs", Allow),
    ("git stash list", Allow),
    ("git -C /var/home/user/repo status", Allow),
    ("git -C ../other-repo log --oneline -5", Allow),
];

/// Cargo safe subcommands.
/// Source: tests/integration.rs — ALLOW: cargo safe subcommands
const ALLOW_CARGO_SAFE: &[(&str, Decision)] = &[
    ("cargo build --release", Allow),
    ("cargo test", Allow),
    ("cargo check", Allow),
    ("cargo clippy", Allow),
    ("cargo fmt", Allow),
    ("cargo --version", Allow),
    ("cargo -V", Allow),
    ("cargo run --bin foo", Allow),
    ("cargo bench", Allow),
    ("cargo clean", Allow),
];

/// gh CLI read-only subcommands.
/// Source: tests/integration.rs — ALLOW: gh CLI read-only
const ALLOW_GH_READONLY: &[(&str, Decision)] = &[
    ("gh pr list", Allow),
    ("gh pr view 123", Allow),
    ("gh pr diff 123", Allow),
    ("gh pr checks 123", Allow),
    ("gh issue list", Allow),
    ("gh issue view 42", Allow),
    ("gh repo view owner/repo", Allow),
    ("gh run list", Allow),
    ("gh status", Allow),
    ("gh search repos rust", Allow),
    ("gh api repos/owner/repo/pulls", Allow),
    ("gh auth status", Allow),
];

/// Mutating commands that should require confirmation.
/// Source: tests/integration.rs — ASK: Mutating commands
const ASK_MUTATING: &[(&str, Decision)] = &[
    ("mkdir -p /tmp/new", Ask),
    ("touch /tmp/newfile", Ask),
    ("mv old.txt new.txt", Ask),
    ("cp src.txt dst.txt", Ask),
    ("ln -s target link", Ask),
    ("chmod 755 script.sh", Ask),
    ("tee /tmp/out.txt", Ask),
    ("curl https://example.com", Ask),
    ("wget https://example.com/file", Ask),
    ("pip install requests", Ask),
    ("npm install express", Ask),
    ("python3 script.py", Ask),
    ("make -j4", Ask),
    ("rm -rf /tmp/junk", Ask),
    ("rmdir /tmp/empty", Ask),
    ("unknown-command --flag", Ask),
];

/// Git mutating subcommands.
/// Source: tests/integration.rs — ASK: git mutating
const ASK_GIT_MUTATING: &[(&str, Decision)] = &[
    ("git push origin main", Ask),
    (
        "GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main",
        Ask,
    ),
    ("git pull origin main", Ask),
    ("git add .", Ask),
    ("git commit -m 'test'", Ask),
    ("git push --force origin main", Ask),
    ("git push -f origin main", Ask),
    ("git push --force-with-lease origin main", Ask),
    ("git -C /some/repo push origin main", Ask),
];

/// Cargo mutating subcommands.
/// Source: tests/integration.rs — ASK: cargo mutating
const ASK_CARGO_MUTATING: &[(&str, Decision)] =
    &[("cargo install ripgrep", Ask), ("cargo publish", Ask)];

/// kubectl mutating subcommands.
/// Source: tests/integration.rs — ASK: kubectl mutating
const ASK_KUBECTL_MUTATING: &[(&str, Decision)] = &[
    ("kubectl apply -f deploy.yaml", Ask),
    ("kubectl delete pod foo", Ask),
    ("kubectl rollout restart deploy/foo", Ask),
    ("kubectl scale --replicas=3 deploy/foo", Ask),
];

/// gh CLI mutating subcommands.
/// Source: tests/integration.rs — ASK: gh CLI mutating
const ASK_GH_MUTATING: &[(&str, Decision)] = &[
    ("gh pr create --title 'Fix'", Ask),
    ("gh pr merge 123", Ask),
    ("gh pr close 123", Ask),
    ("gh pr comment 123 --body 'LGTM'", Ask),
    ("gh issue create --title 'Bug'", Ask),
    ("gh repo create my-repo --public", Ask),
    ("gh release create v1.0", Ask),
    ("gh repo delete my-repo --yes", Ask),
    ("gh secret set MY_SECRET", Ask),
    ("gh auth login", Ask),
];

/// Privilege escalation wrappers (always at least Ask).
/// Source: tests/integration.rs — ASK: Privilege escalation
const ASK_PRIVILEGE_ESCALATION: &[(&str, Decision)] = &[
    ("sudo apt install vim", Ask),
    ("su - root", Ask),
    ("doas pacman -S vim", Ask),
];

/// Version flags on unrecognized commands.
/// Source: tests/integration.rs — ASK: Version flags on unrecognized commands
/// NOTE: unrecognized commands with --version still Ask because the command
/// itself is unknown (version detection only works for known-allowed commands).
const ASK_UNRECOGNIZED_VERSION: &[(&str, Decision)] = &[("rustc --version", Ask), ("node -V", Ask)];

/// Deny-listed commands.
/// Source: tests/integration.rs — DENY
const DENY_BLOCKED: &[(&str, Decision)] = &[
    ("shred /dev/sda", Deny),
    ("dd if=/dev/zero of=/dev/sda", Deny),
    ("eval 'rm -rf /'", Deny),
    ("shutdown -h now", Deny),
    ("reboot", Deny),
    ("halt", Deny),
    ("mkfs.ext4 /dev/sda1", Deny), // dotted-command fallback: mkfs.ext4 -> mkfs
];

/// Redirection escalation: an otherwise-allowed command with output redirection
/// becomes Ask (except /dev/null).
/// Source: tests/integration.rs — Redirection escalation
const REDIR_ESCALATION: &[(&str, Decision)] = &[
    ("ls -la > /tmp/out.txt", Ask),
    ("ls -la >> /tmp/out.txt", Ask),
    ("eza --icons > files.txt", Ask),
    ("kubectl get pods > pods.txt", Ask),
    ("git status > /tmp/s.txt", Ask),
    ("bat file 2> /tmp/err", Ask),
    ("bat file &> /tmp/out", Ask),
    ("cargo build --release > /tmp/log", Ask),
    ("git log > /tmp/log.txt", Ask),
    ("gh pr list > /tmp/prs.txt", Ask),
    ("echo hi >| file.txt", Ask),
    ("cat <> file.txt", Ask),
];

/// /dev/null redirection is non-mutating and preserves Allow.
/// Source: tests/integration.rs — /dev/null redirection (non-mutating)
const REDIR_DEVNULL: &[(&str, Decision)] = &[
    ("ls -la > /dev/null", Allow),
    ("ls -la 2> /dev/null", Allow),
    ("ls -la &> /dev/null", Allow),
    ("cargo test 2> /dev/null", Allow),
    ("git status > /dev/null", Allow),
    // Mixed: /dev/null for stderr but real file for stdout -> Ask
    ("ls -la > /tmp/out 2> /dev/null", Ask),
];

/// FD duplication (2>&1, 1>&2, etc.) is not mutation.
/// Source: tests/integration.rs — fd duplication (NOT mutation)
const FD_DUPLICATION: &[(&str, Decision)] = &[
    ("ls -la 2>&1", Allow),
    ("ls -la 1>&2", Allow),
    ("ls -la >&2", Allow),
    ("ls -la 2>&-", Allow),                 // fd close
    ("ls -la > /tmp/out 2>&1", Ask),        // real redir + dup
    ("cargo test 2>&1 | rg FAILED", Allow), // dup in pipe
    ("ls -la >&3", Ask),                    // dup to custom fd
    ("ls -la 2>&3", Ask),                   // stderr to custom fd
    ("ls -la >&2", Allow),                  // dup to standard fd
];

/// Compound commands: &&, ;, || chains.
/// Source: tests/integration.rs — Compound commands
const COMPOUND_CHAINS: &[(&str, Decision)] = &[
    ("ls -la && rm -rf /tmp", Ask),
    ("ls -la && shred foo", Deny),
    ("ls -la ; eza --icons", Allow),
    ("kubectl get pods ; kubectl get svc", Allow),
    ("tree . && bat README.md", Allow),
    ("kubectl get pods && kubectl delete pod foo", Ask),
    ("ls -la ; dd if=/dev/zero of=disk", Deny),
    ("git log --oneline -5 && gh pr list", Allow),
    ("cd /tmp && ls -la", Allow),
    // Background operator (&)
    ("ls -la & rm -rf /", Ask),
    ("ls -la & echo hello", Allow),
];

/// Pipes.
/// Source: tests/integration.rs — Pipes
const PIPE_CHAINS: &[(&str, Decision)] = &[
    ("kubectl get pods | rg running", Allow),
    ("ls -la | bat", Allow),
    ("eza | unknown-tool", Ask),
    ("cat src/main.rs | grep 'fn ' | wc -l", Allow),
    ("find . -name '*.rs' | xargs grep 'TODO'", Allow),
    ("echo 'checking...' && cat README.md", Allow),
    ("cargo build --release && cargo test", Allow),
    ("cargo fmt && cargo clippy", Allow),
];

/// Command substitution $() and backticks.
/// Source: tests/integration.rs — Command substitution
const SUBSTITUTION: &[(&str, Decision)] = &[
    ("ls $(which cargo)", Allow),
    ("bat $(fd '*.rs' src/)", Allow),
    ("ls $(rm -rf /tmp)", Ask),
    ("ls $(shred foo)", Deny),
    ("echo $(cat /etc/passwd)", Allow),
    ("echo `whoami`", Allow),
    ("ls $(cat $(which foo))", Allow),   // nested substitution
    ("echo '$(rm -rf /)'", Allow),       // single-quoted: not expanded
    ("diff <(sort a) <(sort b)", Allow), // process substitution
    ("ls $(which cargo) && bat $(fd '*.rs')", Allow), // subst in compound
    ("ls $(shred foo) && bat README.md", Deny), // subst deny in compound
    ("echo $(ls $(shred /dev/sda))", Deny), // deeply nested deny
];

/// Quoting: redirections/chains inside quotes are literal, not operators.
/// Source: tests/integration.rs — Quoting
const QUOTING: &[(&str, Decision)] = &[("echo 'hello > world'", Allow), ("echo \"a && b\"", Allow)];

/// Control flow: for, while, if — body commands are evaluated.
/// Source: tests/integration.rs — Control flow (for, while, if)
const CONTROL_FLOW: &[(&str, Decision)] = &[
    ("for i in *; do ls \"$i\"; done", Allow),
    ("for i in *; do rm \"$i\"; done", Ask),
    ("for i in *; do shred \"$i\"; done", Deny),
    ("while true; do echo hello; done", Allow),
    ("if true; then ls; fi", Allow),
    ("if true; then rm foo; fi", Ask),
    (
        "while true; do shred /dev/sda; done <<EOF | cat\nstuff\nEOF",
        Deny,
    ),
    (
        "for f in *; do ls \"$f\"; done <<EOF | grep foo\ndata\nEOF",
        Allow,
    ),
];

/// Wrapper commands: xargs, env, sudo, doas, su, nohup, nice, timeout, time, watch, parallel.
/// Source: tests/integration.rs — Wrapper commands
const WRAPPERS: &[(&str, Decision)] = &[
    ("xargs rm -rf", Ask),
    ("xargs shred", Deny),
    ("xargs grep pattern", Allow),
    ("xargs -0 -I {} rm {}", Ask),
    ("env rm -rf /tmp/test", Ask),
    ("env FOO=bar rm -rf /tmp/test", Ask),
    ("env ls -la", Allow),
    ("env HOME=/tmp ls -la", Allow),
    (
        "env KUBECONFIG=~/.kube/config kubectl apply -f foo.yaml",
        Ask,
    ),
    ("sudo ls", Ask),
    ("sudo shred /dev/sda", Deny),
    ("sudo rm -rf /", Ask),
    ("sudo -u postgres psql", Ask),
    ("doas rm -rf /", Ask),
    ("doas shred /dev/sda", Deny),
    ("su -c rm", Ask),
    ("nohup rm -rf /tmp/test", Ask),
    ("nice -n 10 ls -la", Allow),
    ("timeout 30 rm -rf /tmp/test", Ask),
    ("time ls -la", Allow),
    ("watch kubectl get pods", Allow),
    ("watch rm -rf /tmp/test", Ask),
    ("parallel rm", Ask),
    ("parallel grep pattern", Allow),
    // bare "env" is in ALLOW_READONLY (env with no args is read-only)
    ("sudo", Ask),                  // bare sudo (ask-floor wrapper, no wrapped command)
    ("xargs echo > /tmp/out", Ask), // wrapper with redir
    ("strace ls -la", Allow),
    ("ltrace ls -la", Allow),
];

/// Redirection propagation: list vs. control-flow (issue #36).
/// Source: tests/integration.rs — allow_export_and_assign_before_redirect
const REDIR_PROPAGATION: &[(&str, Decision)] = &[(
    "export FOO=bar && REPO_ID=$(echo test) && cat > /tmp/file",
    Ask,
)];

/// Pipeline with trailing redirect (issue #37).
/// Source: tests/integration.rs — pipeline_with_trailing_redirect_asks
const PIPELINE_REDIR: &[(&str, Decision)] = &[("echo hello | cat > /tmp/file", Ask)];

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 2: Heredoc tests (hand-written, from integration.rs)
// ═══════════════════════════════════════════════════════════════════════════════

/// Heredoc pipe tests — commands piped from heredocs.
/// Source: tests/integration.rs — Heredoc pipe-swallowing regression tests
const HEREDOC_PIPE: &[(&str, Decision)] = &[
    // heredoc piped to kubectl apply
    (
        "cat <<'EOF' | kubectl apply -f -\napiVersion: rbac.authorization.k8s.io/v1\nkind: Role\nmetadata:\n  name: ai-agent\n  namespace: external-secrets\nEOF\n",
        Ask,
    ),
    // heredoc piped to kubectl delete
    (
        "cat <<'EOF' | kubectl delete -f -\napiVersion: v1\nkind: Pod\nEOF\n",
        Ask,
    ),
    // heredoc piped to xargs rm
    ("cat <<'EOF' | xargs rm\nfile1\nfile2\nEOF\n", Ask),
    // heredoc piped to grep (safe)
    (
        "cat <<'EOF' | grep pattern\nsome text\npattern here\nEOF\n",
        Allow,
    ),
    // heredoc piped to xargs shred
    ("cat <<'EOF' | xargs shred\nfile1\nEOF\n", Deny),
    // heredoc piped to eval
    ("cat <<'EOF' | eval\nmalicious command\nEOF\n", Deny),
    // unquoted heredoc piped to kubectl apply
    (
        "cat <<EOF | kubectl apply -f -\napiVersion: v1\nkind: Secret\nEOF\n",
        Ask,
    ),
    // heredoc only, no pipe — cat is safe
    ("cat <<'EOF'\njust printing text\nEOF\n", Allow),
];

/// Heredoc combined with other operators (&&, ;, ||).
/// Source: tests/integration.rs — heredoc_and_dangerous_command, etc.
const HEREDOC_COMPOUND: &[(&str, Decision)] = &[
    ("cat <<'EOF' && rm -rf /tmp/test\nbody\nEOF\n", Ask),
    ("cat <<'EOF' ; kubectl delete pod foo\nbody\nEOF\n", Ask),
    ("cat <<'EOF' || rm -rf /\nbody\nEOF\n", Ask),
];

/// Heredoc substitution expansion.
/// Source: tests/integration.rs — heredoc_unquoted_subst_shred_denies, heredoc_quoted_subst_not_expanded
const HEREDOC_SUBST: &[(&str, Decision)] = &[
    // Unquoted heredoc: bash expands $() -> shred -> DENY
    ("cat <<EOF\n$(shred /dev/sda)\nEOF", Deny),
    // Quoted heredoc: $() is NOT expanded -> cat alone -> ALLOW
    ("cat <<'EOF'\n$(shred /dev/sda)\nEOF", Allow),
];

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 3: Escalate-deny tests (custom registry setting)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Source: tests/integration.rs — escalate_deny tests

/// When escalate_deny=true, DENY -> ASK; ALLOW and ASK unchanged.
const ESCALATE_DENY_CASES: &[(&str, Decision)] = &[
    ("shred /dev/sda", Ask),      // deny -> ask
    ("ls -la", Allow),            // allow unchanged
    ("rm -rf /tmp", Ask),         // ask unchanged
    ("ls -la && shred foo", Ask), // compound with deny -> ask
];

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 4: Substitution with reason assertion (hand-written)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Source: tests/integration.rs — subst_double_quoted_expanded

const SUBST_WITH_REASON: &[(&str, Decision)] = &[("echo \"$(rm -rf /)\"", Ask)];

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 5: Tool-specific unit tests (spec-level, default config)
// ═══════════════════════════════════════════════════════════════════════════════
//
// These test the tool specs directly (not via cc_toolgate::evaluate()),
// but with default config they produce the same results.

/// Git spec unit tests (default config, no env-gating).
/// Source: src/commands/tools/git.rs — tests (default config section)
const GIT_SPEC_DEFAULT: &[(&str, Decision)] = &[
    ("git push origin main", Ask),
    (
        "GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main",
        Ask,
    ), // default has empty config_env
    ("git log --oneline -10", Allow),
    ("git diff HEAD~1", Allow),
    ("git branch -a", Allow),
    ("git status", Allow),
    ("git log > /tmp/log.txt", Ask), // redir escalation
    // Global flag skipping
    ("git -C /some/path status", Allow),
    ("git -C /some/repo log --oneline", Allow),
    ("git -C ../other diff", Allow),
    ("git -C /some/repo push origin main", Ask),
    ("git --no-pager log", Allow),
    ("git -c core.pager=cat status", Allow),
];

/// Cargo spec unit tests (default config).
/// Source: src/commands/tools/cargo.rs — tests
const CARGO_SPEC_DEFAULT: &[(&str, Decision)] = &[
    ("cargo build --release", Allow),
    ("cargo test", Allow),
    ("cargo clippy", Allow),
    ("cargo --version", Allow),
    ("cargo -V", Allow),
    ("cargo install ripgrep", Ask),
    ("cargo publish", Ask),
    ("cargo build --release > /tmp/log", Ask), // redir escalation
];

/// gh spec unit tests (default config).
/// Source: src/commands/tools/gh.rs — tests
const GH_SPEC_DEFAULT: &[(&str, Decision)] = &[
    ("gh pr list", Allow),
    ("gh pr view 123", Allow),
    ("gh status", Allow),
    ("gh api repos/owner/repo/pulls", Allow),
    ("gh pr create --title 'Fix'", Ask),
    ("gh pr merge 123", Ask),
    ("gh repo delete my-repo --yes", Ask),
    ("gh pr list > /tmp/prs.txt", Ask), // redir escalation
];

/// kubectl spec unit tests (default config).
/// Source: src/commands/tools/kubectl.rs — tests
const KUBECTL_SPEC_DEFAULT: &[(&str, Decision)] = &[
    ("kubectl get pods", Allow),
    ("kubectl describe svc foo", Allow),
    ("kubectl logs pod/foo", Allow),
    ("kubectl apply -f deploy.yaml", Ask),
    ("kubectl delete pod foo", Ask),
    ("kubectl get pods > pods.txt", Ask), // redir escalation
];

/// SimpleCommandSpec unit tests.
/// Source: src/commands/simple.rs — tests
/// NOTE: These test the spec directly, not via evaluate(). The spec is
/// constructed with a specific decision, so we note the spec type.
const SIMPLE_SPEC: &[(&str, &str, Decision)] = &[
    ("ls -la", "Allow-spec", Allow),
    ("ls > file.txt", "Allow-spec", Ask), // redir escalation
    ("rm -rf /tmp", "Ask-spec", Ask),
    ("shred /dev/sda", "Deny-spec", Deny),
];

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 6: Env-gated tests (custom config with config_env)
// ═══════════════════════════════════════════════════════════════════════════════
//
// These require a custom config with config_env enabled.
// They test the env-gating mechanism for each tool.

/// Git env-gate config.
/// Source: src/commands/tools/git.rs — spec_with_env_gate()
/// Config: read_only=[status,log,diff,branch], allowed_with_config=[push,pull,add],
///         config_env={GIT_CONFIG_GLOBAL: ~/.gitconfig.ai},
///         force_push_flags=[--force, -f, --force-with-lease]
const GIT_ENV_GATE: &[(&str, Decision)] = &[
    (
        "GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main",
        Allow,
    ),
    ("GIT_CONFIG_GLOBAL=~/.gitconfig git push origin main", Ask), // wrong value
    // ("git push origin main", Ask),  // no env at all — requires clear_git_env (nextest)
    (
        "GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push --force origin main",
        Ask,
    ), // force-push always asks
    (
        "GIT_CONFIG_GLOBAL=~/.gitconfig.ai git commit -m 'test'",
        Ask,
    ), // commit not in allowed_with_config
];

/// Git env-gate tests that require nextest (process env mutation).
/// Source: src/commands/tools/git.rs — env_gate_push_no_config
const GIT_ENV_GATE_NEXTEST: &[(&str, Decision)] = &[
    ("git push origin main", Ask), // no inline env, process env cleared
];

/// Cargo env-gate config.
/// Source: src/commands/tools/cargo.rs — spec_with_env_gate()
/// Config: safe_subcommands=[build,check,test], allowed_with_config=[install,publish],
///         config_env={CARGO_INSTALL_ROOT: /tmp/bin}
const CARGO_ENV_GATE: &[(&str, Decision)] = &[
    ("CARGO_INSTALL_ROOT=/tmp/bin cargo install ripgrep", Allow),
    ("CARGO_INSTALL_ROOT=/usr/local cargo install ripgrep", Ask), // wrong value
    ("cargo install ripgrep", Ask),                               // no env
    ("CARGO_INSTALL_ROOT=/tmp/bin cargo publish", Allow),
    ("cargo build", Allow), // safe, no env needed
];

/// gh env-gate config.
/// Source: src/commands/tools/gh.rs — spec_with_env_gate()
/// Config: read_only=[pr list, pr view, status], mutating=[repo delete],
///         allowed_with_config=[pr create, pr merge],
///         config_env={GH_CONFIG_DIR: ~/.config/gh-ai}
const GH_ENV_GATE: &[(&str, Decision)] = &[
    (
        "GH_CONFIG_DIR=~/.config/gh-ai gh pr create --title 'Fix'",
        Allow,
    ),
    ("GH_CONFIG_DIR=~/.config/gh gh pr create --title 'Fix'", Ask), // wrong value
    ("gh pr create --title 'Fix'", Ask),                            // no env
    ("GH_CONFIG_DIR=~/.config/gh-ai gh pr merge 123", Allow),
    ("gh pr list", Allow), // readonly, no env needed
    ("GH_CONFIG_DIR=~/.config/gh-ai gh repo delete my-repo", Ask), // mutating, not env-gated
];

/// kubectl env-gate config.
/// Source: src/commands/tools/kubectl.rs — spec_with_env_gate()
/// Config: read_only=[get,describe], mutating=[delete],
///         allowed_with_config=[apply,rollout],
///         config_env={KUBECONFIG: ~/.kube/config.ai}
const KUBECTL_ENV_GATE: &[(&str, Decision)] = &[
    (
        "KUBECONFIG=~/.kube/config.ai kubectl apply -f deploy.yaml",
        Allow,
    ),
    (
        "KUBECONFIG=~/.kube/config kubectl apply -f deploy.yaml",
        Ask,
    ), // wrong value
    // ("kubectl apply -f deploy.yaml", Ask),  // no env — requires clear_kubectl_env (nextest)
    ("kubectl get pods", Allow), // readonly, no env needed
    ("KUBECONFIG=~/.kube/config.ai kubectl delete pod foo", Ask), // mutating, not env-gated
];

/// kubectl env-gate tests that require nextest.
/// Source: src/commands/tools/kubectl.rs — env_gate_apply_no_config
const KUBECTL_ENV_GATE_NEXTEST: &[(&str, Decision)] = &[("kubectl apply -f deploy.yaml", Ask)];

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 7: Compound env accumulation tests (eval/mod.rs)
// ═══════════════════════════════════════════════════════════════════════════════
//
// These use a registry with git config_env enabled:
//   allowed_with_config=[push, commit, add], config_env={GIT_CONFIG_GLOBAL: ~/.gitconfig.ai}
//
// Source: src/eval/mod.rs — compound env accumulation tests

/// Compound env accumulation: export/assignment propagation through ;, &&, ||, |.
/// Config: git env-gate (push, commit, add gated on GIT_CONFIG_GLOBAL).
const COMPOUND_ENV_ACCUM: &[(&str, Decision)] = &[
    // Semicolon: unconditional, env propagates
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
        Allow,
    ),
    // And-chain: export is likely_successful, env propagates
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        Allow,
    ),
    // Multiple exports chained
    (
        "export PATH=/usr/bin && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        Allow,
    ),
    // Bare assignment + semicolon
    (
        "GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
        Allow,
    ),
    // Bare assignment + and-chain
    (
        "GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        Allow,
    ),
    // Wrong export value
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.wrong && git push origin main",
        Ask,
    ),
    // Export overridden by later export
    (
        "export GIT_CONFIG_GLOBAL=wrong ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
        Allow,
    ),
    // Echo + export + git push (all likely_successful)
    (
        "echo 'Pushing...' && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        Allow,
    ),
    // Realistic Claude pattern
    (
        "export PATH=/home/user/.cargo/bin:/usr/bin && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && echo 'Pushing...' && git push -u origin feature-branch",
        Allow,
    ),
    // Force push still asks even with correct env
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push --force origin main",
        Ask,
    ),
    // Semicolon after unknown_cmd + export: export accumulates
    // BUT overall is Ask because unknown_cmd is Ask (strictest wins)
    (
        "unknown_cmd ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
        Ask,
    ),
    // Echo (likely_successful) + semicolon + export + git push
    (
        "echo starting ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
        Allow,
    ),
    // echo && export && git push
    (
        "echo 'Pushing...' && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        Allow,
    ),
    // Export + unknown_cmd breaks &&-chain
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && unknown_cmd && git push origin main",
        Ask,
    ),
    // Subshell in export breaks && chain
    (
        "export GIT_CONFIG_GLOBAL=$(cat ~/.gitconfig.ai.path) && git push origin main",
        Ask,
    ),
    // Subshell in echo breaks && chain
    (
        "echo $(some_status_cmd) && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        Ask,
    ),
];

/// Compound env tests that require nextest (process env mutation).
/// Source: src/eval/mod.rs — tests with clear_git_env()
const COMPOUND_ENV_ACCUM_NEXTEST: &[(&str, Decision)] = &[
    // || means git push only runs if export failed -> env not set
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai || git push origin main",
        Ask,
    ),
    // | means subshell boundary -> env doesn't propagate
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai | git push origin main",
        Ask,
    ),
    // || clears accumulated env
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && echo ok || export OTHER=x && git push origin main",
        Ask,
    ),
];

/// Unset interaction with env accumulation (requires nextest for some).
/// Source: src/eval/mod.rs — unset tests
const UNSET_ENV: &[(&str, Decision)] = &[
    // unset only removes the named var, not others
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset OTHER_VAR ; git push origin main",
        Allow,
    ),
    // unset -f (function unset) does NOT remove the variable
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset -f GIT_CONFIG_GLOBAL ; git push origin main",
        Allow,
    ),
];

/// Unset tests that require nextest.
/// Source: src/eval/mod.rs — unset_removes_accumulated_var
const UNSET_ENV_NEXTEST: &[(&str, Decision)] = &[
    // unset removes the accumulated var -> git push can't see it
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset GIT_CONFIG_GLOBAL ; git push origin main",
        Ask,
    ),
];

/// env -i / env - wrapper clears accumulated env for wrapped command.
/// Source: src/eval/mod.rs — env -i tests (require nextest)
const ENV_WRAPPER_CLEAR_NEXTEST: &[(&str, Decision)] = &[
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env -i git push origin main",
        Ask,
    ),
    (
        "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env - git push origin main",
        Ask,
    ),
];

/// env without -i passes accumulated env through.
/// Source: src/eval/mod.rs — env_without_i_passes_accumulated_env
const ENV_WRAPPER_PASSTHROUGH: &[(&str, Decision)] = &[(
    "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env git push origin main",
    Allow,
)];

// ═══════════════════════════════════════════════════════════════════════════════
// TEST RUNNERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Run a table of (command, expected_decision) pairs against default config.
fn assert_table(table: &[(&str, Decision)], category: &str) {
    for (i, (cmd, expected)) in table.iter().enumerate() {
        let actual = decision_for(cmd);
        assert_eq!(
            actual,
            *expected,
            "[{category}#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {actual:?}\n  reason:   {}",
            reason_for(cmd),
        );
    }
}

// ── Default config tables ──

#[test]
fn spec_allow_readonly() {
    assert_table(ALLOW_READONLY, "allow-readonly");
}

#[test]
fn spec_allow_kubectl_readonly() {
    assert_table(ALLOW_KUBECTL_READONLY, "allow-kubectl-readonly");
}

#[test]
fn spec_allow_git_readonly() {
    assert_table(ALLOW_GIT_READONLY, "allow-git-readonly");
}

#[test]
fn spec_allow_cargo_safe() {
    assert_table(ALLOW_CARGO_SAFE, "allow-cargo-safe");
}

#[test]
fn spec_allow_gh_readonly() {
    assert_table(ALLOW_GH_READONLY, "allow-gh-readonly");
}

#[test]
fn spec_ask_mutating() {
    assert_table(ASK_MUTATING, "ask-mutating");
}

#[test]
fn spec_ask_git_mutating() {
    assert_table(ASK_GIT_MUTATING, "ask-git-mutating");
}

#[test]
fn spec_ask_cargo_mutating() {
    assert_table(ASK_CARGO_MUTATING, "ask-cargo-mutating");
}

#[test]
fn spec_ask_kubectl_mutating() {
    assert_table(ASK_KUBECTL_MUTATING, "ask-kubectl-mutating");
}

#[test]
fn spec_ask_gh_mutating() {
    assert_table(ASK_GH_MUTATING, "ask-gh-mutating");
}

#[test]
fn spec_ask_privilege_escalation() {
    assert_table(ASK_PRIVILEGE_ESCALATION, "ask-privilege-escalation");
}

#[test]
fn spec_ask_unrecognized_version() {
    assert_table(ASK_UNRECOGNIZED_VERSION, "ask-unrecognized-version");
}

#[test]
fn spec_deny_blocked() {
    assert_table(DENY_BLOCKED, "deny-blocked");
}

#[test]
fn spec_redir_escalation() {
    assert_table(REDIR_ESCALATION, "redir-escalation");
}

#[test]
fn spec_redir_devnull() {
    assert_table(REDIR_DEVNULL, "redir-devnull");
}

#[test]
fn spec_fd_duplication() {
    assert_table(FD_DUPLICATION, "fd-duplication");
}

#[test]
fn spec_compound_chains() {
    assert_table(COMPOUND_CHAINS, "compound-chains");
}

#[test]
fn spec_pipe_chains() {
    assert_table(PIPE_CHAINS, "pipe-chains");
}

#[test]
fn spec_substitution() {
    assert_table(SUBSTITUTION, "substitution");
}

#[test]
fn spec_quoting() {
    assert_table(QUOTING, "quoting");
}

#[test]
fn spec_control_flow() {
    assert_table(CONTROL_FLOW, "control-flow");
}

#[test]
fn spec_wrappers() {
    assert_table(WRAPPERS, "wrappers");
}

#[test]
fn spec_redir_propagation() {
    assert_table(REDIR_PROPAGATION, "redir-propagation");
}

#[test]
fn spec_pipeline_redir() {
    assert_table(PIPELINE_REDIR, "pipeline-redir");
}

#[test]
fn spec_heredoc_pipe() {
    assert_table(HEREDOC_PIPE, "heredoc-pipe");
}

#[test]
fn spec_heredoc_compound() {
    assert_table(HEREDOC_COMPOUND, "heredoc-compound");
}

#[test]
fn spec_heredoc_subst() {
    assert_table(HEREDOC_SUBST, "heredoc-subst");
}

#[test]
fn spec_subst_with_reason() {
    assert_table(SUBST_WITH_REASON, "subst-with-reason");
}

// ── Tool spec tables (default config, verifies overlap with integration tests) ──

#[test]
fn spec_git_default() {
    assert_table(GIT_SPEC_DEFAULT, "git-spec-default");
}

#[test]
fn spec_cargo_default() {
    assert_table(CARGO_SPEC_DEFAULT, "cargo-spec-default");
}

#[test]
fn spec_gh_default() {
    assert_table(GH_SPEC_DEFAULT, "gh-spec-default");
}

#[test]
fn spec_kubectl_default() {
    assert_table(KUBECTL_SPEC_DEFAULT, "kubectl-spec-default");
}

// ═══════════════════════════════════════════════════════════════════════════════
// ESCALATE-DENY TESTS (custom registry)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn spec_escalate_deny() {
    let config = cc_toolgate::config::Config::default_config();
    let mut registry = cc_toolgate::eval::CommandRegistry::from_config(&config);
    registry.set_escalate_deny(true);

    for (i, (cmd, expected)) in ESCALATE_DENY_CASES.iter().enumerate() {
        let result = registry.evaluate(cmd);
        assert_eq!(
            result.decision, *expected,
            "[escalate-deny#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {:?}\n  reason:   {}",
            result.decision, result.reason,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ENV-GATED TESTS (custom config per tool)
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper: build a git env-gate config and evaluate through KnowledgeSpec.
fn git_env_gate_decision(cmd: &str) -> Decision {
    use cc_toolgate::commands::CommandSpec;
    use cc_toolgate::commands::tools::knowledge::KnowledgeSpec;
    use cc_toolgate::eval::CommandContext;
    use std::collections::HashMap;

    let mut config = cc_toolgate::config::Config::default_config();
    config.git.allowed_with_config = vec!["push".into(), "pull".into(), "add".into()];
    config.git.config_env = HashMap::from([("GIT_CONFIG_GLOBAL".into(), "~/.gitconfig.ai".into())]);
    config.git.force_push_flags = vec!["--force".into(), "-f".into(), "--force-with-lease".into()];

    let spec = KnowledgeSpec::from_config(&config);
    let ctx = CommandContext::from_command(cmd);
    spec.evaluate(&ctx).decision
}

#[test]
fn spec_git_env_gate() {
    for (i, (cmd, expected)) in GIT_ENV_GATE.iter().enumerate() {
        let actual = git_env_gate_decision(cmd);
        assert_eq!(
            actual, *expected,
            "[git-env-gate#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {actual:?}",
        );
    }
}

/// Helper: build a cargo env-gate config and evaluate through KnowledgeSpec.
fn cargo_env_gate_decision(cmd: &str) -> Decision {
    use cc_toolgate::commands::CommandSpec;
    use cc_toolgate::commands::tools::knowledge::KnowledgeSpec;
    use cc_toolgate::eval::CommandContext;
    use std::collections::HashMap;

    let mut config = cc_toolgate::config::Config::default_config();
    config.cargo.allowed_with_config = vec!["install".into(), "publish".into()];
    config.cargo.config_env = HashMap::from([("CARGO_INSTALL_ROOT".into(), "/tmp/bin".into())]);

    let spec = KnowledgeSpec::from_config(&config);
    let ctx = CommandContext::from_command(cmd);
    spec.evaluate(&ctx).decision
}

#[test]
fn spec_cargo_env_gate() {
    for (i, (cmd, expected)) in CARGO_ENV_GATE.iter().enumerate() {
        let actual = cargo_env_gate_decision(cmd);
        assert_eq!(
            actual, *expected,
            "[cargo-env-gate#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {actual:?}",
        );
    }
}

/// Helper: build a gh env-gate config and evaluate through KnowledgeSpec.
fn gh_env_gate_decision(cmd: &str) -> Decision {
    use cc_toolgate::commands::CommandSpec;
    use cc_toolgate::commands::tools::knowledge::KnowledgeSpec;
    use cc_toolgate::eval::CommandContext;
    use std::collections::HashMap;

    let mut config = cc_toolgate::config::Config::default_config();
    config.gh.allowed_with_config = vec!["pr create".into(), "pr merge".into()];
    config.gh.config_env = HashMap::from([("GH_CONFIG_DIR".into(), "~/.config/gh-ai".into())]);

    let spec = KnowledgeSpec::from_config(&config);
    let ctx = CommandContext::from_command(cmd);
    spec.evaluate(&ctx).decision
}

#[test]
fn spec_gh_env_gate() {
    for (i, (cmd, expected)) in GH_ENV_GATE.iter().enumerate() {
        let actual = gh_env_gate_decision(cmd);
        assert_eq!(
            actual, *expected,
            "[gh-env-gate#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {actual:?}",
        );
    }
}

/// Helper: build a kubectl env-gate config and evaluate through KnowledgeSpec.
fn kubectl_env_gate_decision(cmd: &str) -> Decision {
    use cc_toolgate::commands::CommandSpec;
    use cc_toolgate::commands::tools::knowledge::KnowledgeSpec;
    use cc_toolgate::eval::CommandContext;
    use std::collections::HashMap;

    let mut config = cc_toolgate::config::Config::default_config();
    config.kubectl.allowed_with_config = vec!["apply".into(), "rollout".into()];
    config.kubectl.config_env = HashMap::from([("KUBECONFIG".into(), "~/.kube/config.ai".into())]);

    let spec = KnowledgeSpec::from_config(&config);
    let ctx = CommandContext::from_command(cmd);
    spec.evaluate(&ctx).decision
}

#[test]
fn spec_kubectl_env_gate() {
    for (i, (cmd, expected)) in KUBECTL_ENV_GATE.iter().enumerate() {
        let actual = kubectl_env_gate_decision(cmd);
        assert_eq!(
            actual, *expected,
            "[kubectl-env-gate#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {actual:?}",
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// COMPOUND ENV ACCUMULATION (custom registry with git env-gate)
// ═══════════════════════════════════════════════════════════════════════════════

fn compound_env_registry() -> cc_toolgate::eval::CommandRegistry {
    let mut config = cc_toolgate::config::Config::default_config();
    config.git.allowed_with_config = vec!["push".into(), "commit".into(), "add".into()];
    config
        .git
        .config_env
        .insert("GIT_CONFIG_GLOBAL".into(), "~/.gitconfig.ai".into());
    cc_toolgate::eval::CommandRegistry::from_config(&config)
}

#[test]
fn spec_compound_env_accumulation() {
    let registry = compound_env_registry();
    for (i, (cmd, expected)) in COMPOUND_ENV_ACCUM.iter().enumerate() {
        let result = registry.evaluate(cmd);
        assert_eq!(
            result.decision, *expected,
            "[compound-env#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {:?}\n  reason:   {}",
            result.decision, result.reason,
        );
    }
}

#[test]
fn spec_unset_env() {
    let registry = compound_env_registry();
    for (i, (cmd, expected)) in UNSET_ENV.iter().enumerate() {
        let result = registry.evaluate(cmd);
        assert_eq!(
            result.decision, *expected,
            "[unset-env#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {:?}\n  reason:   {}",
            result.decision, result.reason,
        );
    }
}

#[test]
fn spec_env_wrapper_passthrough() {
    let registry = compound_env_registry();
    for (i, (cmd, expected)) in ENV_WRAPPER_PASSTHROUGH.iter().enumerate() {
        let result = registry.evaluate(cmd);
        assert_eq!(
            result.decision, *expected,
            "[env-passthrough#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {:?}\n  reason:   {}",
            result.decision, result.reason,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// NEXTEST-ONLY TESTS (process env mutation)
// ═══════════════════════════════════════════════════════════════════════════════
//
// These tests mutate the process environment and require nextest
// (process-per-test isolation). They are guarded by a nextest check.

fn require_nextest() {
    assert!(
        std::env::var("NEXTEST").is_ok(),
        "this test mutates process env and requires nextest (cargo nextest run)"
    );
}

fn clear_git_env() {
    require_nextest();
    unsafe { std::env::remove_var("GIT_CONFIG_GLOBAL") };
}

fn clear_kubectl_env() {
    require_nextest();
    unsafe { std::env::remove_var("KUBECONFIG") };
}

#[test]
fn spec_compound_env_nextest() {
    clear_git_env();
    let registry = compound_env_registry();
    for (i, (cmd, expected)) in COMPOUND_ENV_ACCUM_NEXTEST.iter().enumerate() {
        let result = registry.evaluate(cmd);
        assert_eq!(
            result.decision, *expected,
            "[compound-env-nextest#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {:?}\n  reason:   {}",
            result.decision, result.reason,
        );
    }
}

#[test]
fn spec_unset_env_nextest() {
    clear_git_env();
    let registry = compound_env_registry();
    for (i, (cmd, expected)) in UNSET_ENV_NEXTEST.iter().enumerate() {
        let result = registry.evaluate(cmd);
        assert_eq!(
            result.decision, *expected,
            "[unset-env-nextest#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {:?}\n  reason:   {}",
            result.decision, result.reason,
        );
    }
}

#[test]
fn spec_env_wrapper_clear_nextest() {
    clear_git_env();
    let registry = compound_env_registry();
    for (i, (cmd, expected)) in ENV_WRAPPER_CLEAR_NEXTEST.iter().enumerate() {
        let result = registry.evaluate(cmd);
        assert_eq!(
            result.decision, *expected,
            "[env-clear-nextest#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {:?}\n  reason:   {}",
            result.decision, result.reason,
        );
    }
}

#[test]
fn spec_git_env_gate_nextest() {
    clear_git_env();
    for (i, (cmd, expected)) in GIT_ENV_GATE_NEXTEST.iter().enumerate() {
        let actual = git_env_gate_decision(cmd);
        assert_eq!(
            actual, *expected,
            "[git-env-gate-nextest#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {actual:?}",
        );
    }
}

#[test]
fn spec_kubectl_env_gate_nextest() {
    clear_kubectl_env();
    for (i, (cmd, expected)) in KUBECTL_ENV_GATE_NEXTEST.iter().enumerate() {
        let actual = kubectl_env_gate_decision(cmd);
        assert_eq!(
            actual, *expected,
            "[kubectl-env-gate-nextest#{i}] command: {cmd:?}\n  expected: {expected:?}\n  actual:   {actual:?}",
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// REASON ASSERTION TESTS
// ═══════════════════════════════════════════════════════════════════════════════
//
// Some tests assert on the reason string, not just the decision.
// These are preserved as hand-written tests since reason assertions
// don't fit cleanly into (command, decision) tables.

/// Double-quoted substitution should mention "subst" in the reason.
/// Source: tests/integration.rs — subst_double_quoted_expanded
#[test]
fn reason_subst_double_quoted_expanded() {
    assert_eq!(decision_for("echo \"$(rm -rf /)\""), Ask);
    let r = reason_for("echo \"$(rm -rf /)\"");
    assert!(
        r.contains("subst"),
        "should show substitution evaluation: {r}"
    );
}

/// Heredoc gh pr create: should have exactly 1 substitution, no unrecognized commands.
/// Source: tests/integration.rs — heredoc_gh_pr_create_with_markdown
#[test]
fn reason_heredoc_gh_pr_create_with_markdown() {
    let cmd = "gh pr create --title \"Fix\" --body \"$(cat <<'EOF'\n## Summary\n- **New:** `config.rs`\n- **Changed:** `eval/mod.rs`\nEOF\n)\"";
    assert_eq!(decision_for(cmd), Ask);
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

/// Heredoc git commit: should have exactly 1 substitution, no unrecognized commands.
/// Source: tests/integration.rs — heredoc_git_commit_with_body
#[test]
fn reason_heredoc_git_commit_with_body() {
    let cmd = "git commit -m \"$(cat <<'EOF'\nFix bug in `parse/shell.rs`\n\nCo-Authored-By: Claude\nEOF\n)\"";
    assert_eq!(decision_for(cmd), Ask);
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

/// Heredoc pipe to kubectl: reason should mention kubectl and pipe operator.
/// Source: tests/integration.rs — heredoc_pipe_kubectl_reason_mentions_kubectl
#[test]
fn reason_heredoc_pipe_kubectl() {
    let cmd = "cat <<'EOF' | kubectl apply -f -\napiVersion: v1\nEOF\n";
    let r = reason_for(cmd);
    assert!(r.contains("kubectl"), "reason should mention kubectl: {r}");
    assert!(r.contains("|"), "reason should mention pipe operator: {r}");
}

/// export/assign before redirect: per-segment reasons should show correct decisions.
/// Source: tests/integration.rs — export_and_assign_before_redirect_segments_not_escalated
#[test]
fn reason_export_assign_before_redirect() {
    let result = cc_toolgate::evaluate("export FOO=bar && REPO_ID=$(echo test) && cat > /tmp/file");
    assert_eq!(
        result.decision, Ask,
        "overall decision must be Ask (cat redirects)"
    );
    let reason = &result.reason;
    assert!(
        reason.contains("export FOO=bar") && reason.contains("ALLOW"),
        "export segment must be ALLOW, got: {reason}"
    );
    assert!(
        reason.contains("variable assignment") && reason.contains("ALLOW"),
        "assignment segment must be ALLOW, got: {reason}"
    );
    assert!(
        reason.contains("[cat]") && reason.contains("ASK"),
        "cat segment must be ASK (redirected), got: {reason}"
    );
    assert!(
        reason.contains("redirection"),
        "cat reason must mention redirection, got: {reason}"
    );
}

/// Pipeline redirect: only last stage should be escalated.
/// Source: tests/integration.rs — pipeline_redirect_only_last_stage_escalated
#[test]
fn reason_pipeline_redirect_only_last_stage() {
    let result = cc_toolgate::evaluate("echo hello | cat > /tmp/file");
    let reason = &result.reason;
    assert!(
        reason.contains("echo hello") && reason.contains("ALLOW"),
        "echo stage must be ALLOW, got: {reason}"
    );
    assert!(
        reason.contains("cat") && reason.contains("ASK"),
        "cat stage must be ASK (redirected), got: {reason}"
    );
}

/// escalate_deny: shred reason should contain "escalated from deny".
/// Source: tests/integration.rs — escalate_deny_turns_deny_to_ask
#[test]
fn reason_escalate_deny() {
    let config = cc_toolgate::config::Config::default_config();
    let mut registry = cc_toolgate::eval::CommandRegistry::from_config(&config);
    registry.set_escalate_deny(true);
    let result = registry.evaluate("shred /dev/sda");
    assert_eq!(result.decision, Ask);
    assert!(result.reason.contains("escalated from deny"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// SIMPLE SPEC UNIT TESTS (directly instantiated)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Source: src/commands/simple.rs — tests

#[test]
fn spec_simple_command_spec() {
    use cc_toolgate::commands::CommandSpec;
    use cc_toolgate::commands::simple::SimpleCommandSpec;
    use cc_toolgate::eval::CommandContext;

    for (i, (cmd, spec_type, expected)) in SIMPLE_SPEC.iter().enumerate() {
        let decision = match *spec_type {
            "Allow-spec" => Allow,
            "Ask-spec" => Ask,
            "Deny-spec" => Deny,
            _ => panic!("unknown spec type: {spec_type}"),
        };
        let spec = SimpleCommandSpec::new(decision);
        let ctx = CommandContext::from_command(cmd);
        let result = spec.evaluate(&ctx);
        assert_eq!(
            result.decision, *expected,
            "[simple-spec#{i}] spec={spec_type} command: {cmd:?}\n  expected: {expected:?}\n  actual:   {:?}",
            result.decision,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PARSE ERROR ESCALATION
// ═══════════════════════════════════════════════════════════════════════════════
//
// Commands with parse errors are fail-closed to ASK minimum.

#[test]
fn parse_error_escalates_to_ask() {
    // Malformed shell input triggers parse errors and escalates to at least Ask
    let result = cc_toolgate::evaluate("echo \"unclosed");
    assert_eq!(
        result.decision, Ask,
        "parse errors should escalate to ASK; got: {:?} ({})",
        result.decision, result.reason
    );
    assert!(
        result.reason.contains("parse errors"),
        "reason should mention parse errors; got: {}",
        result.reason
    );
}

#[test]
fn parse_error_preserves_deny_escalation() {
    // Even with parse errors, a DENY command inside should still be denied
    // (parse error escalates to ASK, but DENY > ASK)
    let result = cc_toolgate::evaluate("shred /dev/sda; echo \"unclosed");
    assert_eq!(
        result.decision, Deny,
        "parse error + deny command should still deny; got: {:?} ({})",
        result.decision, result.reason
    );
}
