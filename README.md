# cc-toolgate

A [PreToolUse hook](https://docs.anthropic.com/en/docs/claude-code/hooks) for Claude Code that gates Bash commands before execution. Every command Claude wants to run is parsed, classified, and either allowed silently, prompted for confirmation, or denied outright.

## Why

Claude Code can run arbitrary shell commands. That's powerful but dangerous — a misguided `rm -rf`, an unauthorized `kubectl apply`, or a `sudo` invocation shouldn't happen without your say-so. cc-toolgate sits between Claude and the shell, evaluating every command against configurable rules.

## Decision model

Every command gets one of three decisions:

| Decision | Behavior | When |
|----------|----------|------|
| **ALLOW** | Runs silently | Read-only commands, safe tools (`ls`, `git status`, `cargo build`) |
| **ASK** | Claude Code prompts you | Mutating commands (`rm`, `git push`), unrecognized commands |
| **DENY** | Blocked outright | Destructive commands (`shred`, `dd`, `mkfs`) |

Redirection on allowed commands (e.g. `echo foo > file.txt`) automatically escalates to ASK.

## How it works

1. Claude Code calls the `Bash` tool with a command string
2. The PreToolUse hook pipes the tool input (JSON) to cc-toolgate on stdin
3. cc-toolgate parses the command: splits compound expressions (`&&`, `||`, `|`, `;`), extracts command substitutions (`$(...)`, backticks), detects heredocs, and identifies redirections
4. Each segment is evaluated against the command registry (built from config)
5. The worst decision across all segments wins
6. cc-toolgate outputs the decision as JSON on stdout
7. Claude Code acts on the decision: proceed, prompt, or block

### Wrapper commands

Commands that execute their arguments (like `sudo`, `xargs`, `env`) are evaluated recursively. The wrapped command is extracted and evaluated, and the final decision is the stricter of the wrapper's floor and the inner command's decision.

```
sudo rm -rf /        → max(ask_floor, deny) = DENY
xargs grep foo       → max(allow_floor, allow) = ALLOW
env FOO=bar rm file  → max(allow_floor, ask) = ASK
```

### Compound commands

Compound expressions are split and each part evaluated independently:

```
git status && rm -rf /tmp/stuff    → max(allow, ask) = ASK
echo hello | kubectl apply -f -   → max(allow, ask) = ASK
```

## Installation

Build from source (requires Rust 2024 edition, i.e. rustc 1.85+):

```bash
cargo build --release
```

The binary is at `target/release/cc-toolgate` (~1.1MB with LTO + strip).

### Hook configuration

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/cc-toolgate",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

## Configuration

cc-toolgate ships with sensible defaults embedded in the binary. Override any part by creating `~/.config/cc-toolgate/config.toml`.

### Merge behavior

User config **merges** with defaults — you only specify what you want to change:

- **Lists** extend defaults (deduplicated)
- **Scalars** override defaults
- **`remove_<field>`** subtracts items from default lists
- **`replace = true`** in any section replaces defaults entirely for that section

### Example user config

```toml
# Move source/. from allow to ask (they execute arbitrary code)
[commands]
remove_allow = ["source", "."]
ask = ["source", "."]

# Auto-allow git push/pull when using a separate AI gitconfig
[git]
allowed_with_config = ["push", "pull", "add", "commit"]
config_env_var = "GIT_CONFIG_GLOBAL"

# Remove cargo run from safe subcommands (it executes arbitrary code)
[cargo]
remove_safe_subcommands = ["run"]
```

### Inspecting effective config

```bash
cc-toolgate --dump-config        # TOML output
cc-toolgate --dump-config json   # JSON output
```

### Escalate deny

Pass `--escalate-deny` to turn all DENY decisions into ASK. Useful when you trust the operator but want visibility:

```json
{
  "command": "/path/to/cc-toolgate --escalate-deny",
  "timeout": 5
}
```

## Command categories

### Simple commands (allow / ask / deny)

Flat name-to-decision mapping. See `config.default.toml` for the full default lists.

### Complex command specs

`git`, `cargo`, `kubectl`, and `gh` have subcommand-aware evaluation with read-only vs. mutating distinctions, flag analysis, and optional env-gated auto-allow.

### Wrapper commands

Commands in the `[wrappers]` section execute their arguments as subcommands. Each has a floor decision:

- **`allow_floor`**: `xargs`, `parallel`, `env`, `nohup`, `nice`, `timeout`, `time`, `watch`, `strace`, `ltrace`
- **`ask_floor`**: `sudo`, `su`, `doas`, `pkexec`

## Logging

Decisions are logged to `~/.local/share/cc-toolgate/decisions.log` (one line per evaluation).

## License

MIT
