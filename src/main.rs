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
    let args: Vec<String> = std::env::args().collect();
    let escalate_deny = args.iter().any(|a| a == "--escalate-deny");

    // --dump-config [json]: print effective config and exit
    if let Some(pos) = args.iter().position(|a| a == "--dump-config") {
        let config = cc_toolgate::config::Config::load();
        let format = args.get(pos + 1).map(|s| s.as_str());
        match format {
            Some("json") => {
                println!("{}", serde_json::to_string_pretty(&config).unwrap());
            }
            _ => {
                println!("{}", toml::to_string_pretty(&config).unwrap());
            }
        }
        return;
    }

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

    // Load config (user override or embedded defaults) and build registry
    let config = cc_toolgate::config::Config::load();
    let mut registry = cc_toolgate::eval::CommandRegistry::from_config(&config);
    if escalate_deny {
        registry.set_escalate_deny(true);
    }
    let result = registry.evaluate(&command);

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
