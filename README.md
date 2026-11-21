# kito-codex

[Codex](https://github.com/openai/codex), the coding assistant, set up to
work through [Kito](https://github.com/KitoOrg/kito). A plugin of Kito:
it writes what Codex reads in a project, and answers Codex before it
changes files.

```console
$ kito plugin add --path plugin
$ cd your-repository
$ kito plugin enable codex
$ kito assistant add codex
Codex is set up for Kito in this repository:
  wrote AGENTS.md
  wrote .codex/config.toml
  wrote .codex/hooks.json
```

`kito assistant remove codex` takes exactly that out again.

## What it writes

- **`AGENTS.md`**: the rules of a project kept by Kito (work in a lane,
  change what the lane declared, know why code is the way it is), between
  two marks, after whatever the project says there itself.
- **`.codex/config.toml`**: Kito's tools, `[mcp_servers.kito]` running
  `kito-mcp`, asked for without a prompt each time.
- **`.codex/hooks.json`**: what Codex runs whatever the model decided:
  - before a patch, `kito codex hook before-edit`: a patch that edits the
    main directory, Kito's own files or something the lane did not
    declare is refused, with what to do instead. One on lines written for
    a reason the lane was not told is held back once, with the reasons;
  - after a shell command, `kito hook after-shell`: what the command
    changed where it should not have;
  - when a session starts, `kito hook session-start`: the rules, and who
    is working on what.

What is there already stays: other servers, other hooks, the project's
own instructions. A server called `kito` that is not Kito's is neither
replaced nor taken out.

## Two things Codex asks you

Codex reads a project's `.codex/` only once you **trust the project**,
and runs a project's hooks only once you have **reviewed them**
(`/hooks`). Until then the rules of `AGENTS.md` are advice, and nothing
holds Codex to them but Kito itself, when the lane is integrated.

`kito` and `kito-mcp` must be on the PATH Codex runs with.

## How a patch is read

Codex changes files with one tool, `apply_patch`, whose patch names no
line: a change is found by the lines around it. Kito is asked about
lines, so the plugin finds them the same way in the file as it is now,
and asks about every file of the patch (`kito hook before-edit --file …
--lines …`). One refusal refuses the patch: Codex applies all of it or
none. A change that cannot be placed, and a file added, deleted or moved
to, is asked about whole.

What a shell command changes is not a patch, and is seen only after it
ran.

## What is not tried

The tests feed the hook what the Codex documentation says Codex sends,
and run it under Kito itself; nothing here has been run under Codex
(0.171 was current when this was written). Not confirmed against it:
the name its shell tool is matched by (`Bash`), and whether it shows the
model what the hooks after a command and at the start of a session
answer.

## Development

```console
$ cargo test
```

The last test wants `kito` and `kitod` in `KITO_BIN` or in
`../kito/target/debug`.

## License

MIT or Apache-2.0, at your option.
