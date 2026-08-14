# Platform notes & caveats

Things that bit, and why the code looks the way it does.

[← back to the README](../README.md)

---

## Linux desktop

**Tauri v2 capabilities are mandatory.** Without
`src-tauri/capabilities/default.json`, `event.listen` is denied and the agent
stream never connects at all.

**`claude` is resolved, not looked up.** Apps launched from a desktop menu
inherit the systemd user session PATH, which typically omits `~/.local/bin` —
exactly where Claude Code installs itself. Relying on PATH means the app works
from a terminal and silently fails from the menu, so the binary is resolved
against known locations and its directory is prepended to the child's PATH.
Screenshot tools are resolved the same way.

**Wayland + NVIDIA.** WebKitGTK's DMABUF renderer crashes the window on open
with `Error 71 (Protocol error)`. `main.rs` sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1` before GTK initialises.

**Screenshots shell out.** WebKitGTK has no `getDisplayMedia`, so the desktop's
own region picker is used instead:

| Order | Tool | Environment |
|---|---|---|
| 1 | `grim` + `slurp` | wlroots compositors |
| 2 | `spectacle` | KDE |
| 3 | `gnome-screenshot` | GNOME |
| 4 | `maim`, `scrot`, `import` | X11 |

First one found wins. Install none of them and the button says what to install.
Cancelling is not an error — the only reliable signal is whether a non-empty
file was written, since exit codes disagree across tools.

---

## Webview

**The CSP has to allow `blob:`.** Attachments are held as object URLs, so the
preview reads them through `img-src` and sending reads them through
`connect-src`. Neither is covered by `default-src`, and a dev server has no CSP
at all — so this only ever breaks in an installed build, silently.

**The clipboard API needs a bridge.** WebKitGTK gates
`navigator.clipboard.writeText` behind a secure context and Tauri's custom
protocol does not qualify, so code-block copy buttons did nothing. The API is
bridged to the Tauri clipboard plugin at startup rather than patching each
component.

**shiki is pinned.** `@streamdown/code` wants 3.x while the top level pulled
4.x; an `overrides` entry in `package.json` resolves it.

---

## Known limits

**Model access is per-account.** If `settings.json` pins `"model": "opus"` and
the target account lacks Opus access, resuming fails.

**Do not open the same session in two tabs.** Two Claude processes writing one
JSONL will corrupt the transcript.

**No plan-mode UI.** Headless mode does not expose it; use
`claude --resume <id>` in a terminal when you need it.

**Completed `Write` calls cannot show a true diff.** The previous file contents
are already gone, so every line renders as an addition and the UI says so. For a
*pending* write the file on disk is still the old version, which is where the
real diffs come from.

**`--strict-mcp-config` drops plugin servers too.** Picking MCP servers for a
chat means only those run — servers a plugin brings along are excluded as well.
The picker says so.

**Rust error messages are Turkish only.** The interface is bilingual; error text
raised in the backend is not, so an English UI can surface a Turkish error.

**Usage policy.** Rotating accounts to work around usage limits may conflict
with Anthropic's usage policies. Your call, your account.
