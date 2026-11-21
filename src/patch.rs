//! What a patch of Codex is about to change.
//!
//! Codex edits files with one tool, `apply_patch`, whose input is a text:
//!
//! ```text
//! *** Begin Patch
//! *** Update File: src/shop.ts
//! *** Move to: src/store.ts
//! @@ export function total
//!    let sum = 0;
//! -  sum += price;
//! +  sum += Math.round(price);
//! *** Add File: notes.md
//! +new
//! *** Delete File: old.md
//! *** End Patch
//! ```
//!
//! It names no line: a change is found by the lines around it. Kito is
//! asked about lines, so they are found here the same way, in the file
//! as it is now.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// One file a patch changes: the lines it replaces (from 1), or, with
/// none, all of it (a file added, deleted, moved to, or a change that
/// could not be placed).
#[derive(Debug, PartialEq, Eq)]
pub struct Edit {
    pub file: PathBuf,
    pub lines: Vec<(u32, u32)>,
}

/// The patch in what Codex says of the tool: its `command`, a text or
/// the words of a command line.
pub fn text_of(tool_input: &Value) -> Option<String> {
    let holds = |text: &&str| text.contains("*** Begin Patch");
    let command = &tool_input["command"];
    let found = command.as_str().filter(holds).or_else(|| {
        let words = command.as_array()?;
        words.iter().filter_map(Value::as_str).find(holds)
    });
    found
        .or_else(|| tool_input["patch"].as_str().filter(holds))
        .or_else(|| tool_input["input"].as_str().filter(holds))
        .map(str::to_owned)
}

/// One change inside an updated file: the lines it expects to find (kept
/// and removed), and which of them it removes.
#[derive(Default)]
struct Hunk {
    old: Vec<String>,
    removed: Vec<bool>,
}

/// Where a hunk is in a file, from line `from` (0-based) on: the index
/// of its first line. Lines are compared without the blanks at their end,
/// as Codex is lenient about them too.
fn place(now: &[&str], hunk: &Hunk, from: usize) -> Option<usize> {
    let same = |at: usize| {
        hunk.old
            .iter()
            .enumerate()
            .all(|(i, line)| now[at + i].trim_end() == line.trim_end())
    };
    let last = now.len().checked_sub(hunk.old.len())?;
    (from..=last)
        .find(|at| same(*at))
        .or_else(|| (0..from.min(last + 1)).find(|at| same(*at)))
}

/// The lines the hunks of one file replace, or none when one of them
/// cannot be placed: the whole file is asked about then.
fn lines_of(now: &str, hunks: &[Hunk]) -> Vec<(u32, u32)> {
    let now: Vec<&str> = now.lines().collect();
    let (mut lines, mut from) = (Vec::new(), 0usize);
    for hunk in hunks.iter().filter(|hunk| !hunk.old.is_empty()) {
        let Some(at) = place(&now, hunk, from) else {
            return Vec::new();
        };
        from = at + hunk.old.len();
        let mut run: Option<(u32, u32)> = None;
        for (i, removed) in hunk.removed.iter().enumerate() {
            let line = (at + i + 1) as u32;
            match (removed, run) {
                (true, Some((first, _))) => run = Some((first, line)),
                (true, None) => run = Some((line, line)),
                (false, Some(done)) => {
                    lines.push(done);
                    run = None;
                }
                (false, None) => {}
            }
        }
        lines.extend(run);
        // Lines put in between others and nothing taken out: asked about
        // the lines they are put next to.
        if !hunk.removed.contains(&true) {
            lines.push(((at + 1) as u32, (at + hunk.old.len()) as u32));
        }
    }
    lines
}

/// The files a patch changes, in its order, and the lines of each. `read`
/// gives a file as it is now.
pub fn edits(patch: &str, cwd: &Path, read: impl Fn(&Path) -> Option<String>) -> Vec<Edit> {
    let at = |path: &str| {
        let path = Path::new(path.trim());
        if path.is_absolute() {
            path.to_owned()
        } else {
            cwd.join(path)
        }
    };
    let mut edits: Vec<Edit> = Vec::new();
    // The file being updated, and its hunks so far.
    let mut updating: Option<(PathBuf, Vec<Hunk>)> = None;
    let close = |updating: &mut Option<(PathBuf, Vec<Hunk>)>, edits: &mut Vec<Edit>| {
        if let Some((file, hunks)) = updating.take() {
            let lines = read(&file)
                .map(|now| lines_of(&now, &hunks))
                .unwrap_or_default();
            edits.push(Edit { file, lines });
        }
    };
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("*** ") {
            if let Some(path) = rest.strip_prefix("Update File:") {
                close(&mut updating, &mut edits);
                updating = Some((at(path), vec![Hunk::default()]));
            } else if let Some(path) = rest
                .strip_prefix("Add File:")
                .or_else(|| rest.strip_prefix("Delete File:"))
            {
                close(&mut updating, &mut edits);
                edits.push(Edit {
                    file: at(path),
                    lines: Vec::new(),
                });
            } else if let Some(path) = rest.strip_prefix("Move to:") {
                // Where it goes is written too, all of it.
                edits.push(Edit {
                    file: at(path),
                    lines: Vec::new(),
                });
            } else if rest.starts_with("End Patch") {
                close(&mut updating, &mut edits);
            }
            // `*** End of File` and `*** Begin Patch` say nothing of lines.
            continue;
        }
        let Some((_, hunks)) = updating.as_mut() else {
            continue;
        };
        if line.starts_with("@@") {
            hunks.push(Hunk::default());
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        match line.chars().next() {
            Some('-') => {
                hunk.old.push(line[1..].to_owned());
                hunk.removed.push(true);
            }
            Some('+') => {}
            // A line that is kept: with its blank in front, or empty.
            Some(' ') => {
                hunk.old.push(line[1..].to_owned());
                hunk.removed.push(false);
            }
            _ => {
                hunk.old.push(line.to_owned());
                hunk.removed.push(false);
            }
        }
    }
    close(&mut updating, &mut edits);
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHOP: &str = "export function total(prices: number[]): number {\n  let sum = 0;\n  for (const price of prices) {\n    sum += price;\n  }\n  return sum;\n}\n\nexport function discount(amount: number, percent: number): number {\n  return amount - (amount * percent) / 100;\n}\n";

    fn of(patch: &str) -> Vec<Edit> {
        edits(patch, Path::new("/repo"), |file| {
            (file == Path::new("/repo/src/shop.ts")).then(|| SHOP.to_owned())
        })
    }

    #[test]
    fn the_patch_is_found_in_what_codex_says() {
        let patch = "*** Begin Patch\n*** End Patch";
        assert_eq!(
            text_of(&json!({ "command": patch })).as_deref(),
            Some(patch)
        );
        assert_eq!(
            text_of(&json!({ "command": ["apply_patch", patch] })).as_deref(),
            Some(patch)
        );
        assert_eq!(text_of(&json!({ "command": "ls -la" })), None);
        assert_eq!(text_of(&json!({})), None);
    }

    #[test]
    fn a_change_is_placed_by_the_lines_around_it() {
        let found = of("*** Begin Patch\n*** Update File: src/shop.ts\n@@ export function total\n   for (const price of prices) {\n-    sum += price;\n+    sum += Math.round(price);\n   }\n*** End Patch\n");
        assert_eq!(
            found,
            [Edit {
                file: "/repo/src/shop.ts".into(),
                lines: vec![(4, 4)]
            }]
        );

        // Two changes in one file, the second after the first.
        let found = of("*** Begin Patch\n*** Update File: src/shop.ts\n@@\n-  let sum = 0;\n+  let sum = 0.0;\n@@\n-  return amount - (amount * percent) / 100;\n-}\n+  return 0;\n+}\n*** End Patch\n");
        assert_eq!(found[0].lines, [(2, 2), (10, 11)]);

        // Lines put in: asked about the lines they go next to.
        let found = of("*** Begin Patch\n*** Update File: src/shop.ts\n@@\n   let sum = 0;\n+  // in cents\n   for (const price of prices) {\n*** End Patch\n");
        assert_eq!(found[0].lines, [(2, 3)]);
    }

    #[test]
    fn what_cannot_be_placed_is_the_whole_file() {
        let found = of("*** Begin Patch\n*** Update File: src/shop.ts\n@@\n-  let sum = 1;\n+  let sum = 2;\n*** End Patch\n");
        assert_eq!(
            found,
            [Edit {
                file: "/repo/src/shop.ts".into(),
                lines: vec![]
            }]
        );
        // A file that is not there to read.
        let found =
            of("*** Begin Patch\n*** Update File: src/cart.ts\n@@\n-a\n+b\n*** End Patch\n");
        assert_eq!(
            found,
            [Edit {
                file: "/repo/src/cart.ts".into(),
                lines: vec![]
            }]
        );
    }

    #[test]
    fn files_added_deleted_and_moved_are_asked_about_whole() {
        let found = of("*** Begin Patch\n*** Add File: notes.md\n+hello\n*** Delete File: /elsewhere/old.md\n*** Update File: src/shop.ts\n*** Move to: src/store.ts\n@@\n-  let sum = 0;\n+  let sum = 1;\n*** End Patch\n");
        assert_eq!(
            found,
            [
                Edit {
                    file: "/repo/notes.md".into(),
                    lines: vec![]
                },
                Edit {
                    file: "/elsewhere/old.md".into(),
                    lines: vec![]
                },
                Edit {
                    file: "/repo/src/store.ts".into(),
                    lines: vec![]
                },
                Edit {
                    file: "/repo/src/shop.ts".into(),
                    lines: vec![(2, 2)]
                },
            ]
        );
    }
}
