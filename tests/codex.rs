//! `kito-codex`: what it writes for Codex in a project, and what it
//! answers Codex before a patch.
//!
//! The last test runs it under Kito itself, and needs `kito`, `kitod` and
//! `kito-mcp` in `KITO_BIN` or in `../kito/target/debug` (`cargo build -p
//! kito-cli -p kitod` there).

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn plugin(root: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_kito-codex"))
        .args(args)
        .env("KITO_REPO", root)
        .output()
        .unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), said)
}

fn text(path: PathBuf) -> String {
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn it_writes_what_codex_reads_and_takes_it_out_again() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (ok, said) = plugin(root, &["assistant", "setup"]);
    assert!(ok, "{said}");
    for file in [
        "wrote AGENTS.md",
        "wrote .codex/config.toml",
        "wrote .codex/hooks.json",
    ] {
        assert!(said.replace('\\', "/").contains(file), "{said}");
    }
    assert!(
        said.contains("to trust this project") && said.contains("`/hooks`"),
        "{said}"
    );

    // The rules, Kito's tools, and the three hooks.
    let rules = text(root.join("AGENTS.md"));
    assert!(
        rules.starts_with("<!-- kito: begin") && rules.contains("## Version control: Kito"),
        "{rules}"
    );
    assert!(
        rules.contains("Work in a lane.") && rules.trim_end().ends_with("<!-- kito: end -->"),
        "{rules}"
    );
    assert_eq!(
        text(root.join(".codex/config.toml")),
        "[mcp_servers.kito]\ncommand = \"kito-mcp\"\ndefault_tools_approval_mode = \"approve\"\n"
    );
    let hooks: Value = serde_json::from_str(&text(root.join(".codex/hooks.json"))).unwrap();
    assert_eq!(
        hooks["hooks"]["PreToolUse"],
        json!([{ "matcher": "apply_patch|Edit|Write", "hooks": [{
            "type": "command",
            "command": "kito codex hook before-edit",
            "timeout": 30,
            "statusMessage": "Asking Kito about the edit",
        }]}])
    );
    assert_eq!(
        hooks["hooks"]["PostToolUse"][0]["matcher"],
        "Bash|exec_command|shell"
    );
    assert_eq!(
        hooks["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
        "kito hook after-shell"
    );
    assert!(hooks["hooks"]["SessionStart"][0].get("matcher").is_none());
    assert_eq!(
        hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "kito hook session-start"
    );

    // Again: nothing to do. Out again: nothing left.
    let (ok, said) = plugin(root, &["assistant", "setup"]);
    assert!(
        ok && said.contains("everything was there already"),
        "{said}"
    );
    let (ok, said) = plugin(root, &["assistant", "remove"]);
    assert!(ok && said.contains("removed AGENTS.md"), "{said}");
    assert!(!root.join("AGENTS.md").exists() && !root.join(".codex").exists());
    let (ok, said) = plugin(root, &["assistant", "remove"]);
    assert!(ok && said.contains("nothing of it was there"), "{said}");
}

#[test]
fn what_the_project_says_itself_is_left_as_it_is() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let own_rules = "# Shop\n\nRun `npm test` before you say it works.\n";
    let own_config = "# Ours.\nmodel = \"gpt-5\"\n\n[mcp_servers.docs]\ncommand = \"docs-mcp\" # the handbook\nargs = [\"--stdio\"]\n";
    let own_hooks = "{\n  \"hooks\": {\n    \"PreToolUse\": [\n      {\n        \"matcher\": \"Bash\",\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"./check.sh\"\n          }\n        ]\n      }\n    ]\n  }\n}\n";
    std::fs::create_dir_all(root.join(".codex")).unwrap();
    std::fs::write(root.join("AGENTS.md"), own_rules).unwrap();
    std::fs::write(root.join(".codex/config.toml"), own_config).unwrap();
    std::fs::write(root.join(".codex/hooks.json"), own_hooks).unwrap();

    let (ok, said) = plugin(root, &["assistant", "setup"]);
    assert!(ok && said.contains("changed AGENTS.md"), "{said}");
    let rules = text(root.join("AGENTS.md"));
    assert!(
        rules.starts_with(own_rules) && rules.contains("## Version control: Kito"),
        "{rules}"
    );
    let config = text(root.join(".codex/config.toml"));
    assert!(
        config.starts_with(own_config)
            && config.contains("[mcp_servers.kito]\ncommand = \"kito-mcp\""),
        "{config}"
    );
    let hooks: Value = serde_json::from_str(&text(root.join(".codex/hooks.json"))).unwrap();
    let before = hooks["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(
        (before.len(), before[0]["hooks"][0]["command"].as_str()),
        (2, Some("./check.sh"))
    );

    // Rules that were changed since are written again, in the same place.
    std::fs::write(
        root.join("AGENTS.md"),
        rules.replace("Work in a lane.", "Work wherever."),
    )
    .unwrap();
    let (ok, said) = plugin(root, &["assistant", "setup"]);
    assert!(
        ok && said.contains("changed AGENTS.md") && !said.contains("config.toml"),
        "{said}"
    );
    assert_eq!(text(root.join("AGENTS.md")), rules);

    // Taken out, the project has what it had.
    let (ok, said) = plugin(root, &["assistant", "remove"]);
    assert!(ok, "{said}");
    assert_eq!(text(root.join("AGENTS.md")), own_rules);
    assert_eq!(text(root.join(".codex/config.toml")), own_config);
    assert_eq!(text(root.join(".codex/hooks.json")), own_hooks);

    // `AGENTS.md` is ZCode's too: rules another of Kito's plugins wrote are
    // not written twice, and rules ZCode still needs are not taken out.
    let theirs = "<!-- kito: begin (written by Kito for the assistants that read this file) -->
## Version control: Kito
<!-- kito: end -->
";
    std::fs::write(root.join("AGENTS.md"), theirs).unwrap();
    std::fs::create_dir_all(root.join(".zcode")).unwrap();
    std::fs::write(
        root.join(".zcode/config.json"),
        "{\"mcp\":{\"servers\":{\"kito\":{\"command\":\"kito-mcp\"}}}}",
    )
    .unwrap();
    let (ok, said) = plugin(root, &["assistant", "setup"]);
    assert!(
        ok && !said.contains("changed AGENTS.md") && !said.contains("wrote AGENTS.md"),
        "{said}"
    );
    let (ok, said) = plugin(root, &["assistant", "remove"]);
    assert!(ok && !said.contains("AGENTS.md"), "{said}");
    assert_eq!(text(root.join("AGENTS.md")), theirs);
    std::fs::remove_dir_all(root.join(".zcode")).unwrap();
    std::fs::remove_file(root.join("AGENTS.md")).unwrap();

    // A server called `kito` that is the project's own is not replaced, and not taken out.
    std::fs::write(
        root.join(".codex/config.toml"),
        "[mcp_servers.kito]\ncommand = \"another\"\n",
    )
    .unwrap();
    plugin(root, &["assistant", "setup"]);
    plugin(root, &["assistant", "remove"]);
    assert_eq!(
        text(root.join(".codex/config.toml")),
        "[mcp_servers.kito]\ncommand = \"another\"\n"
    );

    // What cannot be read as Codex writes it is not touched, and said.
    std::fs::write(root.join(".codex/hooks.json"), "not json").unwrap();
    let (ok, said) = plugin(root, &["assistant", "setup"]);
    assert!(
        !ok && said.contains("is not JSON") && said.contains("by hand"),
        "{said}"
    );
    assert_eq!(text(root.join(".codex/hooks.json")), "not json");
}

#[test]
fn outside_kito_it_says_how_it_is_run() {
    let out = Command::new(env!("CARGO_BIN_EXE_kito-codex"))
        .args(["assistant", "setup"])
        .env_remove("KITO_REPO")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("kito plugin enable codex"));
}

fn kito_bin() -> PathBuf {
    let dir = std::env::var_os("KITO_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../kito/target/debug"));
    let kito = dir.join(format!("kito{}", std::env::consts::EXE_SUFFIX));
    assert!(
        kito.is_file(),
        "{} is not there: this test runs kito-codex under Kito. Build it (`cargo build -p kito-cli -p kitod` in \
         the kito repository) or set KITO_BIN to where `kito` and `kitod` are",
        kito.display()
    );
    dir.canonicalize().unwrap()
}

#[test]
fn codex_is_refused_the_patches_kito_refuses() {
    let bin = kito_bin();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let (root, config) = (base.join("shop"), base.join("config"));
    std::fs::create_dir_all(root.join("src")).unwrap();
    // `kito` is found as Codex would find it: on the PATH.
    let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let kito = |cwd: &Path, args: &[&str], input: Option<&Value>| -> Output {
        let mut child = Command::new(bin.join(format!("kito{}", std::env::consts::EXE_SUFFIX)))
            .args(args)
            .current_dir(cwd)
            .env("PATH", &path)
            .env(
                "KITOD_BIN",
                bin.join(format!("kitod{}", std::env::consts::EXE_SUFFIX)),
            )
            .env("KITO_NO_CLAUDE", "1")
            .env("KITO_AUTHOR", "ada")
            .env("KITO_CONFIG_DIR", &config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        if let Some(input) = input {
            stdin.write_all(input.to_string().as_bytes()).unwrap();
        }
        drop(stdin);
        child.wait_with_output().unwrap()
    };
    let ok = |cwd: &Path, args: &[&str]| -> String {
        let out = kito(cwd, args, None);
        let (said, errors) = (
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(out.status.success(), "kito {args:?}: {said}{errors}");
        said.into_owned()
    };

    ok(&root, &["init"]);
    std::fs::write(root.join(".gitignore"), ".kito/\n").unwrap();
    std::fs::write(
        root.join("src/shop.ts"),
        "export const price = 1;\nexport const tax = 2;\n",
    )
    .unwrap();
    std::fs::write(root.join("README.md"), "# Shop\n").unwrap();
    ok(&root, &["checkpoint", "-m", "first version"]);

    // The plugin as it is installed from: the program and its manifest.
    let source = base.join("plugin");
    std::fs::create_dir_all(&source).unwrap();
    let program = Path::new(env!("CARGO_BIN_EXE_kito-codex"));
    std::fs::copy(program, source.join(program.file_name().unwrap())).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("kito-plugin.toml"),
        source.join("kito-plugin.toml"),
    )
    .unwrap();
    ok(
        &root,
        &["plugin", "add", "--path", source.to_str().unwrap()],
    );
    ok(&root, &["plugin", "enable", "codex"]);
    assert!(ok(&root, &["assistant", "list"]).contains("codex  (plugin codex 0.1.2)"));
    let set_up = ok(&root, &["assistant", "add", "codex"]);
    assert!(
        set_up.contains("Codex is set up for Kito") && set_up.contains("wrote AGENTS.md"),
        "{set_up}"
    );
    assert!(root.join(".codex/hooks.json").is_file());

    ok(
        &root,
        &[
            "lane",
            "open",
            "alice",
            "--intent",
            "a cheaper price",
            "--scope",
            "src/shop.ts",
        ],
    );
    let work = root.join(".kito/lanes/1/work");

    // What Codex says before it applies a patch, and what it is answered.
    let asked = |cwd: &Path, patch: &str| -> (Output, Value) {
        let input = json!({
            "hook_event_name": "PreToolUse",
            "cwd": cwd,
            "tool_name": "apply_patch",
            "tool_input": { "command": patch },
        });
        let out = kito(cwd, &["codex", "hook", "before-edit"], Some(&input));
        let said = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let answer = if said.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&said).unwrap_or_else(|e| panic!("{e}: {said}"))
        };
        (out, answer)
    };
    let cheaper = "*** Begin Patch\n*** Update File: src/shop.ts\n@@\n-export const price = 1;\n+export const price = 0.5;\n*** End Patch\n";
    let refusal = |answer: &Value| {
        answer["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };

    // In the main directory: refused, with what to do instead.
    let (out, answer) = asked(&root, cheaper);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        answer["hookSpecificOutput"]["permissionDecision"], "deny",
        "{answer}"
    );
    assert!(refusal(&answer).contains("lane"), "{answer}");

    // In the lane, on what it declared: nothing is said, and the patch goes on.
    let (out, answer) = asked(&work, cheaper);
    assert!(
        out.status.success() && answer.is_null(),
        "{answer}{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // In the lane, outside what it declared: one file of the patch is enough.
    let both = "*** Begin Patch\n*** Update File: src/shop.ts\n@@\n-export const tax = 2;\n+export const tax = 3;\n*** Update File: README.md\n@@\n-# Shop\n+# The shop\n*** End Patch\n";
    let (_, answer) = asked(&work, both);
    assert_eq!(
        answer["hookSpecificOutput"]["permissionDecision"], "deny",
        "{answer}"
    );
    assert!(
        refusal(&answer).contains("README.md") && refusal(&answer).contains("extend_scope"),
        "{answer}"
    );
    // A file added there, the same.
    let (_, answer) = asked(
        &work,
        "*** Begin Patch\n*** Add File: notes.md\n+hello\n*** End Patch\n",
    );
    assert_eq!(
        answer["hookSpecificOutput"]["permissionDecision"], "deny",
        "{answer}"
    );
    // Kito's own files, never.
    let own = format!(
        "*** Begin Patch\n*** Delete File: {}\n*** End Patch\n",
        root.join(".kito/config.toml").display()
    );
    let (_, answer) = asked(&work, &own);
    assert!(refusal(&answer).contains("Kito's own files"), "{answer}");

    // What is no patch is not Kito's to say anything about.
    let shell = json!({ "cwd": root, "tool_name": "Bash", "tool_input": { "command": "ls" } });
    let out = kito(&root, &["codex", "hook", "before-edit"], Some(&shell));
    assert!(out.status.success() && out.stdout.is_empty());

    // Taken out again by Kito.
    let removed = ok(&root, &["assistant", "remove", "codex"]);
    assert!(removed.contains("removed AGENTS.md"), "{removed}");
    assert!(!root.join(".codex").exists());
    let _ = kito(&root, &["daemon", "stop"], None);
}
