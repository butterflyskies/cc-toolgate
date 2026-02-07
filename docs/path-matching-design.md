# Path-based command matching design

## Current behavior (v3)

`base_command()` extracts the basename: `/usr/bin/ls` → `ls`. Registry lookup
is always by basename. No path-aware matching, no symlink resolution.

## Proposed behavior

Three match levels, evaluated in order of specificity. Most specific match wins.
Within the same level, deny > ask > allow.

### Match levels (most → least specific)

1. **Resolved path**: symlink-resolved canonical path (`std::fs::canonicalize`)
2. **Exact path**: literal path string as given in config/command
3. **Basename**: filename component only (current behavior)

### Path resolution for bare commands

When a command has no `/` (e.g. bare `ls`), we resolve it via PATH lookup
(`which`-equivalent: search each PATH directory for an executable with that
name). This ensures that a path-based deny on `/usr/bin/ls` cannot be bypassed
by simply typing `ls`.

If PATH resolution fails (command not found on disk), only basename matching
applies. This is the common case for shell builtins and functions.

### Lookup algorithm

```
given command word W (e.g. "/usr/local/bin/ls", "ls", "./script.sh"):

1. determine full path P:
   - if W contains '/': P = W
   - else: search PATH for executable W → P (may fail)

2. if P is known:
   a. check P in deny             → DENY
   b. resolve symlinks(P) → R
   c. if R != P, check R in deny  → DENY
   d. check P in allow            → ALLOW
   e. check P in ask              → ASK
   f. if R != P, check R in allow → ALLOW
   g. if R != P, check R in ask   → ASK

3. basename fallback (basename of W):
   a. check in deny  → DENY
   b. check in allow → ALLOW
   c. check in ask   → ASK

4. fallthrough → ASK (unrecognized)
```

Deny checks always happen before allow/ask at each level. This ensures that a
deny on the resolved path cannot be circumvented by any alias — symlink,
basename, or bare command.

### Case table

Assume `/usr/local/bin/ls` is a symlink → `/usr/bin/ls`.
Assume bare `ls` resolves via PATH to `/usr/bin/ls`.

| # | Allow list | Deny list | Command | PATH resolves to | Resolved (canonical) | Decision | Reasoning |
|---|-----------|-----------|---------|-----------------|---------------------|----------|-----------|
| 1 | `ls` | — | `ls` | `/usr/bin/ls` | `/usr/bin/ls` | ALLOW | No path entries match; basename `ls` in allow (step 3b) |
| 2 | `ls` | — | `/usr/bin/ls` | (has `/`) | `/usr/bin/ls` | ALLOW | No path entries match; basename `ls` in allow (step 3b) |
| 3 | `/usr/bin/ls` | — | `/usr/bin/ls` | (has `/`) | `/usr/bin/ls` | ALLOW | Exact path allow (step 2d) |
| 4 | `/usr/bin/ls` | — | `ls` | `/usr/bin/ls` | `/usr/bin/ls` | ALLOW | PATH resolves to `/usr/bin/ls` → path allow (step 2d) |
| 5 | `/usr/bin/ls` | — | `/usr/local/bin/ls` | (has `/`) | `/usr/bin/ls` | ALLOW | Resolved path allow (step 2f) |
| 6 | `ls` | `/usr/bin/ls` | `/usr/bin/ls` | (has `/`) | `/usr/bin/ls` | DENY | Exact path deny (step 2a) |
| 7 | `ls` | `/usr/bin/ls` | `ls` | `/usr/bin/ls` | `/usr/bin/ls` | DENY | PATH resolves → path deny (step 2a) |
| 8 | `ls` | `/usr/bin/ls` | `/usr/local/bin/ls` | (has `/`) | `/usr/bin/ls` | DENY | Resolved path deny (step 2c) |
| 9 | `/usr/local/bin/ls` | `/usr/bin/ls` | `/usr/local/bin/ls` | (has `/`) | `/usr/bin/ls` | DENY | Resolved deny (step 2c) before exact allow (step 2d) |
| 10 | — | `ls` | `/usr/bin/ls` | (has `/`) | `/usr/bin/ls` | DENY | No path entries; basename `ls` in deny (step 3a) |
| 11 | `/usr/bin/ls` | `ls` | `/usr/bin/ls` | (has `/`) | `/usr/bin/ls` | ALLOW | Exact path allow (step 2d) before basename deny (step 3a) |
| 12 | `/usr/bin/ls` | `ls` | `ls` | `/usr/bin/ls` | `/usr/bin/ls` | ALLOW | PATH resolves → path allow (step 2d) before basename deny (step 3a) |
| 13 | `ls` | `ls` | `ls` | `/usr/bin/ls` | `/usr/bin/ls` | DENY | No path entries; basename deny > allow (step 3a) |
| 14 | `/usr/bin/ls` | `/usr/bin/ls` | `/usr/bin/ls` | (has `/`) | `/usr/bin/ls` | DENY | Same path, deny > allow (step 2a) |
| 15 | — | — | `/usr/bin/ls` | (has `/`) | `/usr/bin/ls` | ASK | No matches → fallthrough |
| 16 | `ls` | `/usr/bin/ls` | `ls` | not found | N/A | ALLOW | PATH resolution fails → basename only → `ls` in allow |
| 17 | — | `/usr/bin/ls` | `ls` | not found | N/A | ASK | PATH resolution fails → basename not in any list → fallthrough |

### Key invariants

- **Deny on resolved path is absolute**: No alias (symlink, bare name, PATH)
  can bypass a deny on the real binary (cases 7, 8, 9).
- **PATH resolution closes the bare-name loophole**: Typing `ls` instead of
  `/usr/bin/ls` does not bypass a path-based deny (case 7). Only if PATH
  resolution fails does it fall through to basename (case 16).
- **Path specificity overrides basename**: An explicit path allow/deny takes
  precedence over a basename entry (cases 6, 11, 12).
- **Same-level deny wins**: If the same entry appears in both allow and deny
  at the same specificity, deny wins (cases 13, 14).
- **Filesystem access required**: PATH lookup and symlink resolution both
  require filesystem access. If either fails, skip that phase and continue
  to the next level.

### Config format

Path entries in allow/ask/deny lists are distinguished by containing `/`:

```toml
[commands]
allow = [
    "ls",                    # basename: any ls
    "/usr/bin/approved-tool", # path: this specific binary
]
deny = [
    "shred",                 # basename: any shred
    "/opt/sketchy/binary",   # path: deny this specific binary
]
```

### Edge cases

- **`./script.sh`**: Has `/` so used as-is for path matching. `canonicalize()`
  resolves to absolute path based on CWD. Basename is `script.sh`.
- **Broken symlinks**: `canonicalize()` fails → skip resolved-path checks,
  use exact path + basename only.
- **PATH resolution cost**: One directory scan per PATH entry, stopping at
  first match. Same as what `which` does. Negligible for cc-toolgate's
  invocation frequency (once per command, not in a loop).
- **PATH manipulation**: If PATH is modified (e.g. attacker prepends a dir),
  PATH resolution finds the attacker's binary. But cc-toolgate evaluates the
  command Claude Code sends, not what the shell will actually execute. PATH
  manipulation is outside cc-toolgate's threat model (it's a hook, not a
  sandbox).
- **Shell builtins**: `cd`, `export`, `set`, etc. don't exist on disk. PATH
  resolution fails → basename matching only. This is correct behavior since
  builtins are in the basename allow list.
- **Dotted commands**: `mkfs.ext4` — basename extraction gives `mkfs.ext4`.
  The existing dotted prefix fallback (`mkfs.ext4` → check `mkfs`) applies
  after path matching, at the basename level only.
- **Tilde expansion**: `~/bin/tool` — the `~` is NOT expanded by cc-toolgate
  (it's a shell feature). If the command literally contains `~`, it won't
  match a path entry with the expanded home dir. The config should use
  absolute paths, not tildes.

### Implementation notes

- Path entries in the config are identified by containing `/` at parse time.
  They go into a separate lookup structure (e.g. `HashMap<PathBuf, Disposition>`)
  alongside the existing basename `HashMap<String, Box<dyn CommandSpec>>`.
- PATH resolution: iterate `std::env::var("PATH")`, split on `:`, check each
  `dir/command` with `std::fs::metadata()` for existence + executable bit.
- `canonicalize()` is a single syscall. Cache if needed but probably unnecessary.
- Complex command specs (git, cargo, kubectl, gh) are always basename-matched.
  Path matching applies to the simple allow/ask/deny lists only.
