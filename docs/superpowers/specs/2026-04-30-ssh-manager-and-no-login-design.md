# Phase 1 Design — Login Removal + SSH Manager

**Date**: 2026-04-30
**Status**: Brainstorm complete, pending written-spec review
**Scope**: Phase 1 of Warp OSS personal customization. Phase 2 (folder-centric AI agent grouping) deferred until user evaluates Warp's existing Workspace concept.

## 1. Goals

1. Remove the login UI from the `warp-oss` build so the terminal opens to a usable state without sign-in.
2. Add an SSH session manager: `Cmd+Shift+S` opens a fuzzy-search palette over `ssh_config` hosts. `Enter` connects in a new tab; `Shift+Enter` opens a new window.
3. Allow tagging hosts (tag, color, notes) and adding new Warp-managed hosts via a form, **without modifying** the user's `~/.ssh/config`.

## 2. Non-Goals (Phase 1)

- Cloud-sync of profiles or annotations.
- Folder-centric or Workspace-centric agent grouping (Phase 2).
- Editing existing user `~/.ssh/config` entries from inside Warp (we open `$EDITOR` instead).
- Bidirectional sync between user and Warp ssh_config files.
- ssh_config `Match` block authoring in a structured form (raw textarea handles it).
- Saved authorized_keys / agent-forwarding management (use system `ssh-agent`).

## 3. Login Removal

**Change**: One line in `app/src/bin/oss.rs`. Add `FeatureFlag::SkipFirebaseAnonymousUser` to channel state before the `cfg!(debug_assertions)` block.

```rust
state = state.with_additional_features(&[
    warp_core::features::FeatureFlag::SkipFirebaseAnonymousUser,
]);
```

**Mechanism**: `app/src/root_view.rs:1783` already branches on this flag and routes to `AuthOnboardingState::Terminal`, bypassing all auth UI. `ForceLogin` (line 1770) is unset in OSS.

**Onboarding gate** (`root_view.rs:2258`, `requires_login = !is_logged_in && (ai_enabled || warp_drive_enabled)`): default OSS state does not enable AI/Drive on first run. If the user toggles them inside onboarding, they will hit a sign-in prompt — this is acceptable Phase 1 behavior; user can simply skip the onboarding step.

**Side effects** (accepted):
- AI Agent (cloud), Warp Drive, Shared Sessions: silently unavailable.
- Local shell, settings, themes, SSH manager: fully functional.

**Risk**: Minimal. Flag is part of Warp's own infrastructure; same mechanism is used by internal channels.

**Verification**: `cargo build --bin warp-oss --features gui` → run → no Sign-in modal → terminal usable.

## 4. SSH Manager Architecture

### 4.1 File layout

```
~/.ssh/config                   User-owned. Warp reads, never writes.
~/.warp/ssh_config              Warp-owned. First line: `Include ~/.ssh/config`. Warp-added hosts below.
~/.warp/ssh_annotations.sqlite  Warp-owned annotations DB (or new table in existing Warp DB; see §6).
```

### 4.2 Connection mechanism

```
ssh -F ~/.warp/ssh_config -t <alias>
```

- `-F` makes ssh use Warp's config as entry point.
- `-t` forces TTY allocation for interactive shell.
- ssh recursively resolves the `Include` on line 1, so all of the user's existing config (including any nested includes) is honored.

Spawned via Warp's existing PTY pipeline (`PtyOptions.shell_starter`). No new process management.

**Trade-off** (accepted): running `ssh <warp-only-alias>` from outside Warp will fail unless the user manually adds `Include ~/.warp/ssh_config` to their `~/.ssh/config`. We will document this caveat.

### 4.3 Host discovery

On launch and on file change (file watcher on both files):

1. Parse `~/.warp/ssh_config` (transitively includes `~/.ssh/config`) using the `ssh2-config` crate.
2. Collect `Host` blocks where the pattern is a single concrete alias.
3. Filter out wildcard patterns (`Host *`, `Host *.example.com`) — they are config templates, not connectable hosts.
4. Tag each parsed host with `HostSource::User` (defined in `~/.ssh/config` chain) or `HostSource::Warp` (defined in `~/.warp/ssh_config`, excluding the leading `Include` line).
5. Left-join with annotation table on alias.

### 4.4 ssh_config parsing

Use `ssh2-config` crate. It handles `Host`, `Match`, `Include`, and standard options (`HostName`, `Port`, `User`, `IdentityFile`, `ProxyJump`, etc.).

Edge cases:
- Comments and blank lines: ignored.
- Unrecognized options: passed through; ssh client handles at runtime.
- Parse error: catch at parser boundary, surface a banner with file:line, do not crash. Affected hosts shown with warning icon.

### 4.5 First-run setup

On first invocation of `Cmd+Shift+S` or first visit to Settings → SSH Profiles:

1. If `~/.warp/` does not exist, create it.
2. If `~/.warp/ssh_config` does not exist, create with content:
   ```
   # Warp-managed SSH config. Edit hosts below; do not edit Includes.
   Include ~/.ssh/config
   ```
3. Initialize annotations DB (sqlite file or migration).
4. Silent — no prompt to user. User's `~/.ssh/config` is never touched.

## 5. Data Model

### 5.1 Annotation table (sqlite, via diesel migration)

```sql
CREATE TABLE ssh_host_annotation (
    alias              TEXT    PRIMARY KEY,
    tags               TEXT    NOT NULL DEFAULT '[]',  -- JSON array of strings
    color              TEXT,                            -- color slug; see §5.2
    notes              TEXT,
    last_connected_at  INTEGER,                         -- unix epoch seconds; NULL if never
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL
);
```

`alias` is logical FK to ssh_config Host alias (no DB-level FK). Orphan rows (alias not in any ssh_config) are surfaced in Settings with a one-click "Delete orphan" action.

### 5.2 Color palette (fixed slugs)

| slug | hex | typical use |
|---|---|---|
| `red` | `#e74c3c` | prod |
| `orange` | `#f39c12` | staging |
| `green` | `#2ecc71` | dev |
| `blue` | `#3498db` | personal |
| `purple` | `#9b59b6` | customer |
| `gray` | `#777` | utility / unsorted |

### 5.3 In-memory model

```rust
struct SshHost {
    alias: String,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<PathBuf>,
    proxy_jump: Option<String>,
    raw_config_lines: Vec<String>,   // any non-form options
    source: HostSource,
    annotation: Option<HostAnnotation>,
}

enum HostSource {
    User,   // from ~/.ssh/config or any Include thereof
    Warp,   // from ~/.warp/ssh_config (excluding leading `Include ~/.ssh/config`)
}

struct HostAnnotation {
    tags: Vec<String>,
    color: Option<ColorSlug>,
    notes: Option<String>,
    last_connected_at: Option<DateTime<Utc>>,
}
```

## 6. Components

```
app/src/ssh_manager/
├── mod.rs                  // SshManager singleton, init/teardown
├── parser.rs               // ssh2-config wrapper; emits Vec<SshHost>
├── model.rs                // SshHostsModel — joins parser output + annotation_repo
├── annotation_repo.rs      // CRUD over sqlite
├── connection.rs           // command-line builder; PTY spawn helpers
├── file_watcher.rs         // watches both ssh_config files; debounced refresh
├── actions.rs              // WorkspaceAction extensions
├── picker.rs               // Cmd+Shift+S palette picker view
├── editor.rs               // Add/Edit Warp-owned host modal (form + advanced raw)
└── settings_page.rs        // Settings → SSH Profiles list/manage
```

**Persistence DB choice** (deferred to implementation): If the existing Warp persistence DB is gated on user identity (and OSS-no-login means no DB), we use a separate sqlite file at `~/.warp/ssh_annotations.sqlite`. If the DB is local-first, we add a diesel migration. Decided in implementation phase 0 (probe DB init code).

**Integration points** (from earlier audit):

| File:line | Modification |
|---|---|
| `app/src/workspace/action.rs:99+` | Add `WorkspaceAction::OpenSshPicker`, `WorkspaceAction::ConnectSshHost { alias, target }` |
| `app/src/workspace/view.rs:611+` | Add binding name const `SSH_PICKER_BINDING_NAME = "workspace:ssh_picker"` |
| `app/src/workspace/view.rs` (action handler) | Route SSH actions to `SshManager::*` |
| `app/src/settings_view/mod.rs:188` | Add `SettingsSection::SshProfiles` enum variant |
| `app/src/pane_group/mod.rs:5415` | Accept SSH connection spec; build `shell_starter` from `connection::build_command` |
| `app/src/lib.rs` | Initialize `SshManager` singleton during app boot |
| Default keybindings file | Register `cmd+shift+s` → `workspace:ssh_picker` |

## 7. User Flows

### 7.1 Open picker (`Cmd+Shift+S`)

1. Action → `SshManager::open_picker`.
2. Lazy first-run setup if needed.
3. Refresh model from parser (cheap; ssh_config files are typically <100 hosts).
4. Render palette overlay; rows sorted by `(last_connected_at desc, alias asc)`.
5. Search input fuzzy-matches against `alias + tags + notes`.
6. Each row shows: alias (large) · hostname (gray) · color dot · tags as pills · notes preview (truncated).
7. Bottom row: `+ Add new host…`.
8. `Esc` dismisses.

### 7.2 Connect (`Enter` / `Shift+Enter`)

1. Selected row → action `ConnectSshHost { alias, target }`.
2. `target = Tab` (Enter): call existing `create_session` with `shell_starter = ["ssh", "-F", "<resolved>/ssh_config", "-t", alias]`.
3. `target = Window` (Shift+Enter): spawn new window first, then `create_session` in its initial workspace.
4. After session start, upsert `last_connected_at = now` in annotation row.
5. Picker dismisses.

### 7.3 Add Warp-owned host

1. From picker `+ Add new host…` or Settings → SSH Profiles → `+ Add`.
2. Modal opens with form (alias, hostname, port, user, identity_file) + collapsible "Advanced (raw ssh_config)" textarea + annotation section (tags, color, notes).
3. On save:
   a. Compose Host block.
   b. Validate by writing to a temp file and running `ssh -F <temp> -G <alias>`. On non-zero exit, show inline error highlighting offending line; do not write `~/.warp/ssh_config`.
   c. Append validated block to `~/.warp/ssh_config` with leading newline.
   d. Refresh model.
   e. Annotation upsert into sqlite.

### 7.4 Edit Warp-owned host

Same modal as 7.3, prepopulated. On save, rewrite the matching `Host` block in place inside `~/.warp/ssh_config` (we own this file, so we can rewrite it preserving order of other blocks).

### 7.5 Annotate any host (User-owned or Warp-owned)

1. Right-click row in picker → "Annotate" (or click row in Settings).
2. Annotation-only modal: tags, color, notes.
3. Save → upsert sqlite. ssh_config files unchanged.

### 7.6 Edit ssh_config fields of a User-owned host

Click an external row in Settings → "Open in editor". Spawn `$EDITOR ~/.ssh/config` (use `code -g <path>:<line>` if VS Code is detected). User edits in their editor. File watcher refreshes Warp on save.

### 7.7 Delete

- Warp-owned host: removes Host block from `~/.warp/ssh_config` and orphan annotation; one-click in Settings, confirmation toast.
- User-owned host: not deletable from Warp UI ("Open in editor" instead).
- Annotation only: clears tags/color/notes, leaves ssh_config untouched.

## 8. Error Handling

| Condition | Behavior |
|---|---|
| ssh_config parse error | Banner with file:line; affected hosts show warning icon; non-erroring hosts still listed |
| `~/.warp/` permission denied | Toast on first-run; SSH manager disabled until resolved (manual chmod) |
| Validation `ssh -G` fails on save | Inline error in editor, line-highlighted; do not write file |
| File watcher event burst | Debounce 500 ms; coalesce |
| ssh process exits non-zero | Existing PTY behavior: output visible, tab marked exited; reconnect via picker |
| Annotation orphan | Surface in Settings with "Delete orphan" |
| `ssh2-config` panic on malformed input | Catch at parser boundary; report parse-error banner; no crash |
| Already-connected session whose alias was removed externally | Existing PTY keeps running unaffected; new connections via picker no longer find that alias |
| `~/.warp/ssh_config` corrupt (e.g. user manually broke it) | Same parse-error path; offer "Reset to default" button |

## 9. Keybindings

Default registrations in Warp's keybindings:

| Action | Binding | Scope |
|---|---|---|
| `workspace:ssh_picker` | `cmd+shift+s` (macOS) / `ctrl+shift+s` | Workspace (global) |
| `ssh_picker:connect_tab` | `enter` | Picker focus |
| `ssh_picker:connect_window` | `shift+enter` | Picker focus |
| `ssh_picker:dismiss` | `escape` | Picker focus |

User-customizable via existing keybindings UI (auto-discovered from action registry).

## 10. Testing

### 10.1 Unit

- `parser.rs`: parse fixture ssh_config files (with Includes, Match, comments, errors). Assert host list and field extraction.
- `annotation_repo.rs`: CRUD round-trip; upsert idempotency; orphan detection.
- `connection.rs`: command-line builder produces correct argv for tab/window targets.

### 10.2 Integration

- End-to-end: launch picker → search → connect → assert PTY started with expected argv.
- File watcher: modify `~/.ssh/config` while Warp running → assert model refresh.
- First-run setup: empty `~/.warp/` → invoke picker → assert files created.

### 10.3 Manual smoke (acceptance criteria)

1. `cargo build --bin warp-oss --features gui` → succeeds.
2. Launch → no Sign-in modal.
3. `Cmd+Shift+S` → palette opens; lists hosts from real `~/.ssh/config`.
4. Select a host → `Enter` → tab opens with shell on remote.
5. `Shift+Enter` on a host → new window opens.
6. Add a Warp-only host via form → connect → tab opens.
7. Annotate a host with red color + tag "prod" → restart Warp → annotation persists.
8. Quit and relaunch → no login prompt; SSH manager state intact.

## 11. File Checklist (Implementation Order)

| # | File | Action |
|---|---|---|
| 1 | `app/src/bin/oss.rs` | Add `SkipFirebaseAnonymousUser` flag |
| 2 | `Cargo.toml` (workspace + `app/Cargo.toml`) | Add `ssh2-config` dependency |
| 3 | `crates/persistence/migrations/YYYYMMDDHHMMSS_ssh_annotations/{up,down}.sql` (or new sqlite file init code) | Create annotation table |
| 4 | `app/src/ssh_manager/parser.rs` | New |
| 5 | `app/src/ssh_manager/annotation_repo.rs` | New |
| 6 | `app/src/ssh_manager/model.rs` | New |
| 7 | `app/src/ssh_manager/connection.rs` | New |
| 8 | `app/src/ssh_manager/file_watcher.rs` | New |
| 9 | `app/src/ssh_manager/actions.rs` | New |
| 10 | `app/src/ssh_manager/mod.rs` | Wire singleton |
| 11 | `app/src/lib.rs` | Initialize `SshManager` singleton |
| 12 | `app/src/workspace/action.rs` | Add SSH actions |
| 13 | `app/src/workspace/view.rs` | Action handler + binding name constants |
| 14 | `app/src/ssh_manager/picker.rs` | Picker view |
| 15 | `app/src/ssh_manager/editor.rs` | Add/edit modal |
| 16 | `app/src/ssh_manager/settings_page.rs` | Settings page |
| 17 | `app/src/settings_view/mod.rs` | Add `SshProfiles` section |
| 18 | `app/src/pane_group/mod.rs` | Plumb SSH `shell_starter` override |
| 19 | Default keybindings file | Register `cmd+shift+s` |
| 20 | Tests + manual smoke | Verify acceptance criteria |

## 12. Open Items (Resolved in Implementation Phase 0)

- Whether to use the existing Warp persistence DB or a new sqlite file. Probe DB initialization code to confirm whether DB requires authenticated user.
- Exact path of "default keybindings file" — confirm by grepping `default_keybindings` or similar in `app/src/`.
- Picker visual styling — adopt existing palette styling as baseline; row layout follows command palette conventions.
- Editor modal styling — adopt existing Warp settings modal pattern.

These do not affect the design; they affect placement only.
