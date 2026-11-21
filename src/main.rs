//! `kito-codex`: Codex, the coding assistant, set up to work through Kito.
//!
//! A plugin of Kito (ADR 0005 there). In a repository that enabled it:
//!
//! - `kito assistant add codex` (which runs `kito-codex assistant setup`)
//!   writes what Codex reads in a project: the rules in `AGENTS.md`, Kito's
//!   tools and the hooks in `.codex/`;
//! - `kito assistant remove codex` takes exactly that out again;
//! - `kito codex hook before-edit` is what Codex runs before it changes
//!   files: it reads the patch Codex is about to apply, asks Kito about
//!   every file in it, and has Codex refuse the patch when Kito says no.

mod patch;
mod setup;

use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// The repository, as Kito says it to its plugins.
const ENV_REPO: &str = "KITO_REPO";

/// What Codex gave the hook of itself: the model it runs, and its session.
struct Said<'a> {
    model: Option<&'a str>,
    session: Option<&'a str>,
}

/// Asks Kito whether one edit may be made: `Ok(None)`, it may; `Ok(Some)`,
/// it may not, and why; `Err`, nobody could be asked. Kito is told that
/// Codex asks, and on what model: the lane the edit is in says it.
fn may_edit(edit: &patch::Edit, said: &Said) -> Result<Option<String>, String> {
    let asked = |who: bool| {
        let mut command = Command::new("kito");
        command
            .args(["hook", "before-edit", "--file"])
            .arg(&edit.file);
        for (from, to) in &edit.lines {
            command.arg("--lines").arg(format!("{from}-{to}"));
        }
        if who {
            command.arg("--assistant=codex");
            command.args(said.model.map(|model| format!("--model={model}")));
            command.args(said.session.map(|session| format!("--session={session}")));
        }
        // Kito places the file by where it is, whatever directory this runs in.
        command.stdin(Stdio::null()).output().map_err(|e| {
            format!("`kito` could not be run ({e}): is it on the PATH Codex runs with?")
        })
    };
    let mut out = asked(true)?;
    // A Kito before 0.25 does not take who asks: it is asked as it was.
    if out.status.code() == Some(2) && String::from_utf8_lossy(&out.stderr).contains("--assistant")
    {
        out = asked(false)?;
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    match out.status.code() {
        Some(0) => Ok(None),
        Some(2) => Ok(Some(said)),
        _ => Err(said),
    }
}

/// `kito codex hook before-edit`: what Codex says about the tool it is
/// about to run is on the standard input; the answer is the one Codex
/// reads to refuse it.
fn before_edit() -> ExitCode {
    let mut text = String::new();
    let _ = std::io::stdin().read_to_string(&mut text);
    let input: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let cwd = PathBuf::from(input["cwd"].as_str().unwrap_or("."));
    let Some(patch) = patch::text_of(&input["tool_input"]) else {
        return ExitCode::SUCCESS;
    };
    // Codex gives every hook the model it runs and its session.
    let said = Said {
        model: input["model"].as_str(),
        session: input["session_id"].as_str(),
    };
    for edit in patch::edits(&patch, &cwd, |file| std::fs::read_to_string(file).ok()) {
        match may_edit(&edit, &said) {
            Ok(None) => {}
            // The whole patch is refused: Codex applies all of it or none.
            Ok(Some(why)) => {
                let answer = json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": why,
                    }
                });
                println!("{answer}");
                return ExitCode::SUCCESS;
            }
            // Not checked now: it is when the lane is integrated.
            Err(why) => {
                eprintln!("kito-codex: the edit was not checked: {why}");
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}

fn repository() -> Result<PathBuf, String> {
    std::env::var_os(ENV_REPO)
        .filter(|root| !root.is_empty())
        .map(PathBuf::from)
        .filter(|root| root.is_dir())
        .ok_or_else(|| {
            "this is a plugin of Kito: `kito plugin enable codex` in a repository, then `kito assistant add codex`"
                .to_owned()
        })
}

fn said(root: &Path, done: Vec<setup::Done>, nothing: &str) {
    if done.is_empty() {
        println!("{nothing}");
    }
    for one in done {
        let path = one.path.strip_prefix(root).unwrap_or(&one.path);
        println!("  {} {}", one.what, path.display());
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let done = match words.as_slice() {
        ["hook", "before-edit"] => return before_edit(),
        ["assistant", "setup"] => repository().and_then(|root| {
            let done = setup::setup(&root)?;
            println!("Codex is set up for Kito in this repository:");
            said(&root, done, "  everything was there already");
            println!("Two things are Codex's to ask you, once, when you start it here:");
            println!("  - to trust this project: without that it reads nothing of `.codex/`;");
            println!("  - to review the hooks (`/hooks`): until then it does not run them, and the rules of");
            println!("    AGENTS.md are advice only.");
            println!("`kito` and `kito-mcp` must be on the PATH Codex runs with.");
            Ok(())
        }),
        ["assistant", "remove"] => repository().and_then(|root| {
            let done = setup::remove(&root)?;
            println!("Codex is no longer set up for Kito in this repository:");
            said(&root, done, "  nothing of it was there");
            Ok(())
        }),
        // One last time, where the plugin is disabled (`[cleanup]`, Kito
        // 0.23): what it wrote in the project comes out with it.
        ["cleanup", "repository"] => repository().and_then(|root| {
            for one in setup::remove(&root)? {
                println!("{} {}", one.what, one.path.strip_prefix(&root).unwrap_or(&one.path).display());
            }
            Ok(())
        }),
        ["cleanup", _] => Ok(()),
        ["--version"] => {
            println!("kito-codex {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => Err("it is `kito assistant add codex`, `kito assistant remove codex`, or, for Codex itself, `kito \
                  codex hook before-edit`"
            .to_owned()),
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("kito-codex: {why}");
            ExitCode::from(1)
        }
    }
}
