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

That workaround has a price: compositing falls back to the CPU, so *any*
continuous animation is expensive. A permanently looping mascot and a
never-cancelled requestAnimationFrame were holding an idle window at 21–28% of
a core — most of it in the main process, doing the compositing. With idle
motion handed to a single CSS keyframe and the frame loop stopping when it has
nothing to type, the same idle window sits at 3–5%.

**`cargo build --release` does not produce a working app.** Asset embedding is
behind the `custom-protocol` feature, which only `tauri build` turns on; a plain
cargo release build silently falls back to `devUrl` and renders a blank window
against a dev server that is not running. It looks like a working binary — it
starts, opens a window, and uses no CPU, which is exactly what a broken one
looks like too.

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

**Pasted images are not always files, and sometimes there is no paste event at
all.** WebKitGTK does not reliably mark a pasted image as `kind: "file"`, so
extraction keys off the MIME type and reads both `files` and `items`. But the
event itself can also simply not arrive for image content — which is why every
Ctrl+V now schedules a check 150 ms later and asks the system clipboard if
nothing was attached in the meantime. Trusting the paste event whenever focus
was in the textarea left the common case with no working path at all.

**Reading the clipboard image happens in Rust.** The JS route went plugin →
raw RGBA → `ImageData` → canvas → blob, four places to fail silently. One
command does it now: `arboard` reads the image and the `png` crate encodes it.
Verified against a real Wayland clipboard — `wl-copy` an image, then
`cargo test --test clipboard_probe -- --ignored`, which asserts the returned
bytes carry a PNG signature.

**Flex and grid children need `min-w-0`.** Their default minimum is the
content's intrinsic width, so one unbreakable string — a long file path in the
recent list — pushed the dialog's content past the panel it was drawn in. The
panel stayed 512px, the content spilled out over the page behind it, and the
close button ended up looking mid-panel. Measured at 255px of overflow before
the fix.

**shiki is pinned.** `@streamdown/code` wants 3.x while the top level pulled
4.x; an `overrides` entry in `package.json` resolves it.

---

## Known limits

**A chat's MCP choice is persisted.** `~/.claude-accounts/session-prefs.json`
maps session id to the selected servers, so the choice survives closing the app
and reopening the conversation from the list. Without it the selection lived
only in the tab and every restart silently fell back to the global config.

**Changing MCP means restarting the session.** `--mcp-config` is a start-up
flag, and `/mcp enable|disable|reconnect` is refused in headless mode — it
answers *"MCP controls aren't available right now"* because it wants the
interactive terminal. So the panel stops the process and starts it again with
`--resume` and the new config. Verified end to end: same session id, MCP
emptied, and the model still recalled a word from before the restart.

**`system/init` carries live MCP status.** It is re-emitted at the start of
every turn with each server's name and state (`pending` → `connected`,
`needs-auth`), which is where the panel's indicators come from — not a guess.

**The session cache is on disk.** Transcripts are append-only, so an unchanged
`(path, mtime, size)` means unchanged content. Persisting the parsed result
turns a 401 ms rescan at every launch into 5 ms. The flag marking whether a
transcript holds a real conversation has to be persisted with it — leave it out
and every cached session comes back looking empty and vanishes from the list.

**Local-command transcripts are not sessions.** Every `claude -p` call writes a
transcript, including the app's own `/usage` poll. Those are deleted after the
query, and the scanner skips any transcript whose only user records are meta or
`<command-…>` wrappers — otherwise the session list fills with entries titled
after a caveat banner.

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
