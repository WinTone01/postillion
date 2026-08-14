# Postillion

**Switch Claude accounts without losing the conversation.**

A desktop client for Claude Code. Pick up any past session under a different
account and keep going — same context, same history, no re-explaining.

Built with Tauri, Rust and React.

---

## Why this works

Claude Code keeps conversation context on your disk, not on a server:

```
~/.claude/projects/<cwd-slug>/<sessionId>.jsonl
```

These are append-only JSONL transcripts. The API is **stateless per turn** — the
client resends the entire message history on every request, so there is no
server-side conversation object. Prompt caching is only a cost optimisation
(5 min / 1 h TTL, scoped to your org).

The key detail: **transcripts carry no account identity.** Records hold only
`sessionId`, `cwd`, `gitBranch` and timestamps. Switching accounts therefore
doesn't break context — you just miss the cache on the first turn.

> This does **not** apply to claude.ai web chats. Those live on the server and
> are tied to your account. Postillion covers Claude Code sessions only.

## Features

| | |
|---|---|
| **Cross-account resume** | Same transcript, different account, context intact |
| **Chat UI** | Token-by-token streaming, collapsible thinking blocks |
| **Code diffs** | `Edit` / `Write` calls render as real line diffs, not raw JSON |
| **Screenshots** | Capture a region, paste, drop or attach — images go straight to the model |
| **Usage meter** | Session and weekly limits per account, right in the sidebar |
| **Batched approvals** | Approve everything pending at once, or allow a tool for the session |
| **Permission prompts** | Approve tool calls inline, with "always allow" shortcuts |
| **Slash commands** | Autocomplete, sourced live from the running agent |
| **Model & effort** | Both switchable mid-session |
| **MCP, plugins, skills** | Add, remove and toggle from the settings panel |
| **Command palette** | `Ctrl+K` to jump between sessions and accounts |
| **Turkish and English** | Follows the system locale, overridable in settings |

## Install

Requires [Claude Code](https://claude.com/claude-code) on your `PATH`, Node 18+
and a Rust toolchain.

```bash
git clone https://github.com/WinTone01/postillion
cd postillion
./scripts/install.sh
```

The script rebuilds the release binary, then installs it to `~/.local` along
with a desktop entry and icons — no root, no packaging tools. Rebuilding is the
default so an install always ships current sources; pass `--no-build` to reuse
an existing binary. Use `--system` to install to `/usr/local`, or
`--prefix PATH` for somewhere else.

Removal:

```bash
./scripts/uninstall.sh
```

Only files the installer created are removed. `~/.claude` — your sessions,
credentials and settings — is never touched. Extra accounts in
`~/.claude-accounts` survive too unless you pass `--purge-accounts`.

### Development

```bash
npm install
npm run tauri dev      # dev server with hot reload
cd src-tauri && cargo test
```

## How it works

### Account isolation — nothing is moved

Your existing `~/.claude` stays exactly where it is and becomes the `default`
account, acting as the source of shared data. Extra accounts symlink back to it:

```
~/.claude/                       <- default account, owns the shared data
├── projects/                    <- transcripts, single copy
├── plugins/  skills/
├── settings.json  history.jsonl
└── .credentials.json

~/.claude-accounts/<name>/
├── .claude.json                 <- real file, per-account
├── .credentials.json            <- written by `claude auth login`; never touched by us
├── projects      -> ~/.claude/projects
├── plugins       -> ~/.claude/plugins
├── skills        -> ~/.claude/skills
├── settings.json -> ~/.claude/settings.json
└── history.jsonl -> ~/.claude/history.jsonl
```

Your plain `claude` command keeps working, untouched.

### The `CLAUDE_CONFIG_DIR` trap

Extra accounts set `CLAUDE_CONFIG_DIR`. The default account **must not**.

The reason is subtle. When `CLAUDE_CONFIG_DIR` is set, Claude looks for
`.claude.json` *inside* that directory. In a default install the real file sits
at `~/.claude.json` (home root) while `.credentials.json` sits inside
`~/.claude/` — an asymmetry. So passing `CLAUDE_CONFIG_DIR=~/.claude` starts
Claude against an empty shadow config at `~/.claude/.claude.json`: credentials
still resolve, but every project trust approval is gone and the trust dialog
comes back.

`tests/integration.rs` carries a regression test for exactly this.

### Seeding a new account

A fresh `.claude.json` is born empty. When seeding, identity fields
(`oauthAccount`, `userID`, `machineID`, account-scoped caches) are stripped while
the `projects` key is preserved — it holds `hasTrustDialogAccepted`,
`allowedTools` and `mcpServers` for every project you have used.

Writes honour Claude's own lock protocol: a **directory** named
`.claude.json.lock` (`mkdir` is atomic on POSIX, so it works as a mutex).

### Driving the agent

Rather than mirroring a terminal, Claude runs headless:

```
claude -p
  --input-format stream-json --output-format stream-json
  --include-partial-messages --verbose
  --permission-prompt-tool stdio
  --permission-mode manual
  [--resume <sessionId>]
```

`--permission-prompt-tool stdio` is **not listed in `--help`**, but it is what
backs the Agent SDK's `canUseTool` callback. Without it permission requests are
silently denied. With it, the CLI asks and waits:

```jsonc
// CLI -> client
{"type":"control_request","request_id":"...","request":{
  "subtype":"can_use_tool","tool_name":"Write",
  "input":{},
  "permission_suggestions":[{"type":"setMode","mode":"acceptEdits"}],
  "tool_use_id":"toolu_..."}}

// client -> CLI
{"type":"control_response","response":{"subtype":"success","request_id":"...",
  "response":{"behavior":"allow"}}}
```

`permission_suggestions` is what powers the "always allow" button.

### Slash commands and the model list

Both come from the `initialize` control request. The CLI exposes them nowhere
else:

```jsonc
{"type":"control_request","request_id":"cs-initialize",
 "request":{"subtype":"initialize","hooks":{}}}
// response: { commands: [...], models: [...], agents: [...] }
```

Effort has no control request of its own — `/effort <level>` is sent as an
ordinary user message and handled locally by the CLI.

### Loading history

`claude --resume` restores a session **model-side only**; it does not replay past
messages to stdout (verified: zero output with stdin closed). History is
therefore read from the transcript on disk. Those records share the same shape as
live stream events, so a single reducer handles both paths.

## Architecture

| Layer | File | Responsibility |
|---|---|---|
| Paths & validation | `src-tauri/src/paths.rs` | Layout, path-traversal defence |
| Account lifecycle | `src-tauri/src/accounts.rs` | Create / seed / repair / delete, locking |
| Transcript scanning | `src-tauri/src/sessions.rs` | JSONL parsing, head+tail sampling, cache |
| Agent driver | `src-tauri/src/agent.rs` | stream-json, permission control channel |
| Catalog | `src-tauri/src/catalog.rs` | Models, MCP, plugins, marketplaces, skills |
| Stream adapter | `src/lib/claude-stream.ts` | Claude events to UI model |
| UI | `src/components/` | React, shadcn/ui, AI Elements |

### Scanning 400 MB of transcripts

Two optimisations keep session listing responsive:

1. **Head + tail sampling.** Files over 1 MiB are read 128 KB from the start and
   256 KB from the end. Title records repeat throughout the file and *last one
   wins*, so the tail is enough; `sessionId` and `cwd` live at the head.
2. **(path, mtime, size) cache.** For append-only files an unchanged triple
   means unchanged content.

Measured: **108 sessions in 913 ms** across 412 MB.

Sub-agent transcripts (`<sessionId>/subagents/agent-*.jsonl`) are skipped on
purpose — they are not standalone sessions and cannot be resumed.

### Stream adapter

Deltas and complete messages are never merged. The `assistant` event is the
single source of truth; `stream_event` deltas only feed the "typing" preview and
are discarded once the full message lands. Otherwise the same text accumulates
twice.

One more wrinkle: Claude splits a single assistant message across several records
that share one `message.id` — up to six in real transcripts. They are merged,
otherwise React drops duplicate keys and half the conversation vanishes.

## Platform notes

- **Tauri v2 capabilities are mandatory.** Without
  `src-tauri/capabilities/default.json`, `event.listen` is denied and the agent
  stream never connects.
- **`claude` is resolved, not looked up.** Apps launched from a desktop menu
  inherit the systemd user session PATH, which typically omits `~/.local/bin` —
  exactly where Claude Code installs itself. Relying on PATH means the app works
  from a terminal and silently fails from the menu, so the binary is resolved
  against known locations and its directory is prepended to the child's PATH.
- **The UI is bilingual, the backend is not.** Interface strings go through
  `t()` in `src/lib/i18n.ts`, keyed by the Turkish source text the way gettext
  keys by the source string. `npm run check:i18n` fails on a key that is used
  but not translated, which is what keeps the two in step. Error messages
  raised in Rust are still Turkish only.
- **Usage comes from `/usage`, parsed.** There is no structured API for the
  limit percentages. The slash command works in headless mode, costs zero
  tokens, and takes about three seconds, so it is polled and cached. Only the
  active account can be measured — `claude` reads one shared credentials file —
  so other accounts show their last reading with its age.
- **The CSP has to allow `blob:`.** Attachments are held as object URLs, so
  the preview reads them through `img-src` and sending reads them through
  `connect-src`. Neither is covered by `default-src`, and a dev server has no
  CSP at all — so this only breaks in an installed build, silently.
- **Screenshots shell out.** WebKitGTK has no `getDisplayMedia`, so the desktop's
  own region picker is used instead: `grim`+`slurp`, then `spectacle`,
  `gnome-screenshot`, `maim`, `scrot`, `import` — first one found wins. Install
  none of them and the button reports what to install.
- **Wayland + NVIDIA.** WebKitGTK's DMABUF renderer crashes the window on open
  with `Error 71 (Protocol error)`. `main.rs` sets
  `WEBKIT_DISABLE_DMABUF_RENDERER=1` before GTK initialises.
- **shiki is pinned.** `@streamdown/code` wants 3.x while the top level pulled
  4.x; an `overrides` entry in `package.json` resolves it.

## Caveats

- **Model access is per-account.** If `settings.json` pins `"model": "opus"` and
  the target account lacks Opus access, resuming fails.
- **Do not open the same session in two tabs.** Two Claude processes writing one
  JSONL will corrupt the transcript.
- **No plan-mode UI.** Headless mode does not expose it; use
  `claude --resume <id>` in a terminal when you need it.
- **Completed `Write` calls cannot show a true diff.** The previous file contents
  are already gone, so every line renders as an addition and the UI says so.
- Rotating accounts to work around usage limits may conflict with Anthropic's
  usage policies.

## Name

A *postillion* rode the lead horse at a post station. Horses were swapped and the
rider changed — the message kept moving.

## Disclaimer

Not affiliated with Anthropic. "Claude" is a trademark of Anthropic.
