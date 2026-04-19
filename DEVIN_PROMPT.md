# Devin AI — Phase 2 Implementation Prompt

> **DO NOT** analyze, audit, or re-read the codebase from scratch.
> All analysis is DONE. All context is in the files listed below.
> Your job: **IMPLEMENT Phase 2** from `DEVELOPMENT_PLAN.md`.

---

## 📂 Context Files (READ THESE FIRST)

| File | What it contains |
|------|-----------------|
| `DEVELOPMENT_PLAN.md` | Full 5-phase roadmap. Phase 1 is ✅ DONE. You implement Phase 2. |
| `PROJECT_MEMORY.json` | Complete system architecture, modules, events, tools, settings, cancel behavior. |
| `FULL_SYSTEM_AUDIT.md` Section 6 | Detailed AI Provider Architecture design (ProviderMode, routing, failure matrix). |
| `FULL_SYSTEM_AUDIT_ADDENDUM.md` | 9 additional findings (items 2.2–2.4 are Phase 2 scope). |

---

## ✅ Phase 1 — ALREADY DONE (do not redo)

These fixes are already committed (`15dcc10`, `7ca666e`, `b522507`):

- `call_executor_with_fallback` now has real Ollama → OpenRouter fallback (`ai.rs:591-626`)
- `reviewer_messages` now includes tool results with ✓/✗ (`ai.rs:1135-1178`)
- `build_executor_messages` has sliding window (last 20 turns) (`ai.rs:259-306`)
- `parse_plan_json` uses balanced bracket counting (`controller.rs:758-813`)
- `plan_goal` retries once on JSON parse failure (`controller.rs:721-755`)
- `build_task_message` injects prior task summaries (`controller.rs:630-686`)
- `AppState.settings` is `RwLock` (all call sites use `.read()`/`.write()`)

---

## 🎯 Phase 2 — YOUR TASK: Multi-Provider Routing System

### Task 1: `ProviderMode` enum in `desktop/src-tauri/src/settings.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    Cloud,
    Local,
    Hybrid,
}
```

Add to `Settings` struct:
```rust
#[serde(default = "default_provider_mode")]
pub provider_mode: ProviderMode,
#[serde(default)]
pub planner_model: String,
#[serde(default)]
pub reviewer_model: String,
#[serde(default)]
pub executor_model: String,
```

Default: `Hybrid` when `openrouter_api_key` is non-empty, else `Local`.

### Task 2: `Provider` enum + `call_model` dispatch in `desktop/src-tauri/src/ai.rs`

```rust
#[derive(Debug, Clone, Copy)]
enum Provider {
    OpenRouter,
    Ollama,
}

fn resolve_provider(settings: &Settings, role: Role) -> (Provider, Option<Provider>) {
    match settings.provider_mode {
        ProviderMode::Cloud => (Provider::OpenRouter, None),
        ProviderMode::Local => (Provider::Ollama, None),
        ProviderMode::Hybrid => match role {
            Role::Planner | Role::Reviewer => (Provider::OpenRouter, Some(Provider::Ollama)),
            Role::Executor => (Provider::Ollama, Some(Provider::OpenRouter)),
        },
    }
}
```

Build `call_model` that:
1. Calls `resolve_provider` to get (primary, fallback)
2. Tries primary provider
3. On failure, if fallback exists, emits `ai:error` event and tries fallback
4. On both fail, returns combined error string

### Task 3: Replace ALL direct provider calls

In `run_chat_turn` (`ai.rs`), replace:
- Line ~831: `stream_openrouter(...)` for planner → `call_model(..., Role::Planner, ...)`
- Line ~880: `call_executor_with_fallback(...)` → `call_model(..., Role::Executor, ...)`
- Line ~1000: `stream_openrouter/stream_ollama` for reviewer → `call_model(..., Role::Reviewer, ...)`

Remove `call_executor_with_fallback` function entirely (replaced by `call_model`).
Remove the `let use_planner = !settings.openrouter_api_key.is_empty();` line — routing is now handled by `resolve_provider`.

### Task 4: `probe_openrouter` command in `ai.rs`

Mirror `probe_ollama`. Hit `https://openrouter.ai/api/v1/models` with the configured key.
Return `{ reachable: bool, key_valid: bool, error: Option<String> }`.
Register in `lib.rs` invoke_handler.

### Task 5: Raise health probe timeout

In `check_executor` and `probe_ollama`: change `Duration::from_secs(3)` → `Duration::from_secs(10)`.

### Task 6: Add provider metadata to `ai:step` events

In `emit_step` / `finish_step`, include `provider` and `model` fields:
```json
{ "index": 0, "role": "executor", "title": "...", "status": "running",
  "provider": "ollama", "model": "deepseek-coder:6.7b" }
```

### Task 7: `pending_confirms` → `tokio::sync::Mutex`

In `desktop/src-tauri/src/lib.rs`:
```rust
pub pending_confirms: tokio::sync::Mutex<HashMap<String, oneshot::Sender<bool>>>,
```
Update all `.lock().unwrap()` on `pending_confirms` in `tools.rs` to `.lock().await`.

### Task 8: Settings UI — Provider Mode selector

In `desktop/frontend/src/Settings.tsx`, add:
- Radio group: Cloud / Local / Hybrid (recommended)
- Per-role model fields (planner_model, reviewer_model, executor_model)
- "Test OpenRouter" button that calls `probe_openrouter`

### Task 9: `docs/PROVIDER_ROUTING.md`

Document the routing system: modes, fallback rules, failure matrix, settings config.

### Task 10: Update `DEVELOPMENT_PLAN.md`

Mark Phase 2 items as ✅ as you complete them.

---

## ⚠️ CONSTRAINTS

- **Rust files**: `desktop/src-tauri/src/` — follow existing code style (4-space indent, `tracing::warn`, `serde_json::json!`)
- **Frontend files**: `desktop/frontend/src/` — follow existing style (TypeScript, functional components)
- **DO NOT touch** `src/` directory (read-only research snapshot)
- **DO NOT** re-analyze or re-audit — all findings are in `FULL_SYSTEM_AUDIT.md`
- **DO NOT** create new modules — add to existing files unless absolutely necessary
- **Test**: run `cargo check` in `desktop/src-tauri/` after Rust changes
- **Settings backward compat**: new fields must have `#[serde(default)]` so existing configs don't break

---

## 📋 Completion Checklist

```
[ ] ProviderMode enum + settings fields added
[ ] call_model dispatch + resolve_provider implemented
[ ] All direct stream_ollama/stream_openrouter calls replaced
[ ] call_executor_with_fallback removed
[ ] probe_openrouter command added + registered
[ ] Health probe timeout raised to 10s
[ ] Provider metadata in ai:step events
[ ] pending_confirms → tokio::sync::Mutex
[ ] Settings UI updated with Provider Mode
[ ] docs/PROVIDER_ROUTING.md written
[ ] DEVELOPMENT_PLAN.md Phase 2 items marked ✅
[ ] cargo check passes
[ ] No regressions in existing tests
```

---

## 🔑 Key Architecture Decisions (already made — just implement)

1. **Hybrid is default** when OpenRouter key exists — matches current implicit behavior
2. **Executor stays local-first** in Hybrid — only 2 cloud calls per task (planner + reviewer)
3. **Fallback is per-call**, not per-task — if Ollama fails on one iteration, try OpenRouter for that iteration
4. **Empty per-role model** = use the mode's default (e.g., `planner_model: ""` in Hybrid → `openrouter_model`)
5. **`call_model` is the single entry point** — no more direct `stream_ollama`/`stream_openrouter` calls anywhere

---

*Start implementing Task 1 immediately. Do not ask clarifying questions — everything is specified above.*
