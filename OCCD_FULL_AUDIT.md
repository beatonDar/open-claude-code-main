# Open Claude Code Desktop — Full System Audit, Architecture Validation, AI Routing Design & UX Transformation Plan

**Scope:** `open-claude-code-main/desktop/` (Tauri 2 + React frontend). The top-level `src/` is the read-only Claude/Codex research snapshot and is out of scope per `PROJECT_PLAN.md §0`.

**Reviewer posture:** Senior AI Systems Engineer + Product Architect + UX Designer + Code Auditor. Everything below is grounded in the actual files in this repo; no invented modules.

---

## 1. Executive Summary

The desktop product is a **small, disciplined controlled-autonomous coding system**. For a research-grade snapshot, it is already punching above its weight on the fundamentals:

- **Sandbox, cancel, tree-kill, typed cancel reasons, atomic memory writes, bounded per-task traces, circuit breaker, RAII goal-running guard** — all implemented and unit-tested (`cancel.rs`, `tools.rs`, `trace.rs`, `controller.rs`, `ai.rs` each carry `#[cfg(test)]` modules).
- **Grounding is real**: the project scanner (`project_scan::scan_project` + `project_context_summary`) is injected as a second system message to every agent role on every turn, and the executor/reviewer prompts explicitly forbid inventing files or languages not in that context.
- **Write-gate path bypass is closed**: `tools::write_would_change_existing_file` now resolves through `fs_ops::resolve` — the same resolver used by `write_file` — so leading-slash paths can no longer skip `autonomous_confirm_irreversible`.

What's still weak is mostly **product surface** rather than core correctness:

- The "hybrid" AI layer is effectively **planner-on-OpenRouter, everything-else-on-Ollama**, with **no real cross-provider fallback** (`call_executor_with_fallback` is a misleading name — its body is `stream_ollama(...)` only).
- There is **no explicit provider-mode toggle** (`cloud | local | hybrid`) — the mode is implicitly derived from "is an OpenRouter key present?".
- The UI is **informationally correct but visually noisy**: Planner / Executor / Reviewer streams each produce their own bubble, with no reasoning-vs-summary separation and no collapse affordance — it reads like a debug log, not a product.
- **History is not compacted** — long sessions will silently exceed the executor model's context window.

None of these are critical breakage. The system is **production-capable for a single-user, local-first workflow**; it is **not yet production-grade as a shippable AI workspace** until the routing layer is made explicit and the UX gets a Thinking/Summary discipline.

Final score: **7.4 / 10 overall**. Detail in §4.

---

## 2. Technical Audit (End-to-End)

### 2.1 Backend (Rust / Tauri)

| Area | File(s) | Status | Notes |
|------|---------|--------|-------|
| Path sandbox | `fs_ops.rs:19-75` | **Solid** | Canonicalizes root and target, strips leading separators, walks non-existent paths via canonical parent, rejects anything outside root. Re-used by the write-gate (`tools.rs:306-316`) — previously a bypass. |
| File size guard | `fs_ops.rs:126-134` | **OK** | 2 MiB `read_file` cap. No chunked-read tool → large source files become opaque. |
| Cancel primitive | `cancel.rs` | **Solid** | Lock-free `AtomicBool + AtomicU8 + Notify`, typed `CancelReason`, async `cancelled()` future usable in `select!`, `link_from` for parent-child propagation, well-tested. |
| Subprocess lifecycle | `tools.rs:run_cmd_impl` + deny-list | **Solid** | `process_group(0)` on Unix / `CREATE_NEW_PROCESS_GROUP` on Windows, SIGTERM→grace→SIGKILL / `taskkill /T /F`, deny-list rejects `rm -rf /`, `sudo`, `curl \| sh`, fork bomb, `dd of=/dev/`, `>/dev/sda`, `> /etc/…`. |
| Confirm pipeline | `tools.rs:await_user_confirmation` | **OK** | 10-min oneshot timeout, cancel-aware `select!`, stale-entry cleanup. Minor: `confirm_cmd` is sync — safe today, but if ever made async remember this state lock. |
| Streaming SSE | `ai.rs:stream_openrouter` / `stream_ollama` | **Solid** | `biased` `select!` so cancel always wins the race; NDJSON for Ollama, SSE `data:` frames for OpenRouter, tool-call accumulator finalizes arguments once the stream ends. |
| Multi-agent loop | `ai.rs:run_chat_turn` | **Solid** | Planner (optional OpenRouter) → Executor (Ollama tool loop, up to `min(max_iterations, 16)` iterations) → Reviewer (1 corrective retry). No planner re-entry mid-execution — the outer `'outer: loop` only restarts the *executor* on reviewer `NEEDS_FIX`. |
| Controller | `controller.rs:start_goal` | **Solid** | RAII `goal_running` guard, project scan first, planner-driven JSON task plan (`{tasks:[{description}]}`) with heuristic fallback, sequential task execution, per-task timeout, global goal timeout (trips **both** tokens with `CancelReason::Timeout`), circuit breaker, exponential backoff (capped 30 s). |
| Trace | `trace.rs` | **Solid** | Bounded to 200 entries × 4 KiB/field, pins entry 0 (`User`) on truncation, typed variants, `skip_serializing_if` on diff. |
| Memory | `memory.rs` | **Solid** | Temp-file + `fsync` + rename, 4 MiB ceiling, schema-versioned with v1→v2 migration, per-turn update. |
| Settings | `settings.rs` | **OK** | Sensible defaults (task 600 s, goal 7200 s, retry base 1 s, circuit 5). Gap: no `provider_mode` / `routing_strategy` field; `openrouter_api_key` presence is the implicit mode flag. |
| Project scan | `project_scan.rs` | **Solid** | Ignore-dir list, depth 4, entry cap 2000, parses `package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`, records workspace markers, emits `project_context_summary` for every agent. |

### 2.2 Frontend (React)

| Area | File(s) | Status | Notes |
|------|---------|--------|-------|
| Shell | `App.tsx` | **OK** | Four-pane layout (Explorer / Goal & Tasks / Chat / Execution). Watcher lifecycle cleanly tied to `projectDir`. Health-check chips refresh on Settings close. |
| Chat | `components/Chat.tsx` | **Weak (UX)** | Streams tokens per-role into *separate* bubbles — correct mechanically, but with Planner + Executor + Reviewer each getting a bubble plus a "thinking…" fallback, the conversation reads like a log. No reasoning collapse, no Final-Answer emphasis. |
| Execution | `components/Execution.tsx` | **OK** | Step timeline (collapses duplicate indices via `Map`), inline diff renderer, role badges. Missing: ability to filter by role, group consecutive tool calls, or collapse completed steps. |
| Task panel | `components/TaskPanel.tsx` | **Solid** | Handles event reordering (late `task:update` before `task:added` → synthesises a row), dedupes on `task:added`, reconciles stale `running` trees on load, caps failures to 10 newest. Visually dense but behaviourally correct. |
| Confirm overlay | `components/ConfirmCmd.tsx` | **OK** | Intercepts `ai:confirm_request`, resolves via `confirm_cmd`. |
| State model | React `useState` + per-component effects | **OK at scale** | No global store; event subscriptions live in each component. Works for current size; will fragment as soon as a fifth pane is added. |

### 2.3 AI / routing layer

| Aspect | Reality | Gap |
|--------|---------|-----|
| Planner provider | OpenRouter when key is non-empty, otherwise skipped | No explicit "cloud mode disables Ollama" or "hybrid preferred-provider" switch. |
| Executor provider | **Always Ollama.** `call_executor_with_fallback` name is misleading — body is `stream_ollama(...)` only. | No cross-provider fallback. If Ollama is down the entire turn fails even when an OpenRouter key is available. |
| Reviewer provider | OpenRouter if key set, else Ollama | Implicit, not user-configurable. |
| Model-not-loaded handling | Surfaces as HTTP error → step marked failed, turn aborts | No auto-pull / auto-pick / degrade. |
| Retry on transient 5xx / network | None at request level (only at task level) | A flaky 502 from OpenRouter silently disables planning for the turn. |

### 2.4 Tool usage correctness

- `list_dir` — **real**; shallow, filters `.git`, `node_modules`, `target`, `dist`, `.next` (`fs_ops.rs:103-108`).
- `read_file` — **real**; sandboxed, 2 MiB cap, UTF-8 only (binary files fail noisily which is good).
- `write_file` — **real**; sandboxed, diff returned, gated by `autonomous_confirm_irreversible` when the target exists and content differs, using the same resolver as the write path.
- `run_cmd` — **real**; deny-list + allow-list + confirm modal; children in a new process group; `tokio::select!` races cancel / timeout / exit; tree-kill on cancel; on Unix SIGTERM → 300 ms grace → SIGKILL; on Windows `taskkill /T /F` sweep.

**No fake execution paths exist.** The executor cannot synthesize tool results; every tool call goes through `tools::execute_safe` / `tools::execute_run_cmd_gated`, emits `ai:tool_call` / `ai:tool_result`, and feeds the actual output back as a `tool` role message in the next iteration.

### 2.5 Issues & Risks

**Critical**
- *None.* The most recently-fixed item (write-gate path bypass) closed the last critical destructive-write hole.

**High**
1. **Misleading "fallback" helper.** `ai::call_executor_with_fallback` (ai.rs:574) promises planner-first with Ollama fallback; body is Ollama-only. Either rename or actually implement cross-provider degradation. This is the single biggest correctness gap relative to the README's stated posture.
2. **No explicit provider-mode setting.** Product needs `provider_mode: "cloud" | "local" | "hybrid"` and `routing_strategy: "smart" | ...` (see §6). Today mode is inferred from the presence of an API key.
3. **No request-level retry / circuit on provider errors.** Transient network blips are surfaced as full-turn failures (planner) or hard errors (executor). A short `retry(attempts=2, backoff=300 ms, jittered)` wrapper at `stream_openrouter` / `stream_ollama` would be cheap and meaningful.
4. **Executor context-window overflow is unguarded.** `build_executor_messages` appends full history and every `tool` result (including diffs concatenated after output, `ai.rs:969-973`). Long sessions or repeated large diffs will eventually exceed the model's window and produce truncation errors from the provider. No sliding-window / summarisation / compaction is applied.
5. **Mutex panic poisoning.** All `AppState` locks (`settings`, `goal_running`, `pending_confirms`) use `.lock().unwrap()`. If any holder panics, the whole app becomes unusable for the remainder of its lifetime. Either recover (`unwrap_or_else(PoisonError::into_inner)`) or swap to `parking_lot::Mutex`.

**Medium**
6. `state.cancelled.reset()` at the top of every `run_chat_turn` (ai.rs:763) wipes a user's "Stop" press if it landed just before the next task in a goal starts. Goal-level cancel is safe (separate `goal_cancelled` token, also tripped by `cancel_goal`), but per-turn cancel semantics during autonomous runs are subtle enough that the Stop button should be labelled accordingly or combined with Goal-cancel in the UI.
7. Task reviewer prefers planner when available and falls back to executor (`ai.rs:984-988`, `controller.rs` similar). This cross-provider preference is *hidden* — moving it behind an explicit `routing_strategy` knob removes surprise.
8. No retrieval over `task_history` / `failures_log`. `PROJECT_MEMORY.json` grows a rich corpus but the executor/planner never see it. A small "relevant past decisions" excerpt in `project_context_summary` would measurably reduce repeated mistakes.
9. No automated lint / typecheck CI. `bun run typecheck` and `cargo check` exist locally but aren't enforced.
10. `Task.status` is `String` even though `TaskStatus` enum exists. Serialize the enum directly (serde tag = "lowercase") to remove the dual source of truth.

**Low**
11. `let _ = app.emit(...)` throughout. Defensible (emit only fails when the window is closed) but makes debugging harder — consider a thin logging wrapper.
12. Non-UTF-8 paths are `to_string_lossy`'d silently — acceptable for a dev tool, worth documenting.
13. `Settings::default()` is `#[serde(default)]`-derived. Works but an explicit `impl Default for Settings` would make seed-defaults testable.
14. Inline-diff renderer in `Execution.tsx` splits on `\n` without virtualization — very large diffs can pin a slow render. Virtualize at ~500 lines.

**Hidden risks (hunted-for, none found)**
- **Runaway loops** — outer loop bounded by `MAX_REVIEWER_RETRIES=1` and `max_iterations ≤ 16`. Safe.
- **Planner re-entry mid-execution** — does **not** happen; only the executor restarts on `NEEDS_FIX`. Safe.
- **Fake execution** — every tool call round-trips through Rust. Safe.
- **Cancel leaking across turns** — `reset()` is explicit at turn start. Documented.
- **Destructive-write bypass via leading `/`** — closed by the fs_ops::resolve rewrite. Safe.

---

## 3. Execution Quality Evaluation

| Property | Verdict | Evidence |
|----------|---------|----------|
| Executes on real file reads | **Yes** | Every `read_file` / `list_dir` hits disk via `fs_ops::*`; the executor prompt explicitly forbids "mentally reviewing imaginary reports" (ai.rs:88-90). |
| State continuity | **Yes, turn- and task-scoped** | `PROJECT_MEMORY.json` is rewritten atomically on every successful turn (`memory::update_turn_memory`); active task tree persisted after each task; `task_history` archives completed trees. |
| Avoids planner re-entry mid-execution | **Yes** | `run_chat_turn` runs the planner exactly once (Phase 1). Reviewer `NEEDS_FIX` restarts the **executor** only (ai.rs:1013-1025). |
| Grounded outputs (anti-hallucination) | **Yes, strongly** | `project_context_summary` injected per turn as a system message; executor/reviewer prompts explicitly forbid out-of-context languages/files. |
| Long-running reliability | **Yes, for 600 s per task** | Task timeout 600 s default, goal timeout 7200 s, circuit breaker on 5 consecutive failures, exponential backoff capped at 30 s. |

**Where it's weakest:** *nothing about execution itself is broken* — weaknesses are on the outer edges (context-window compaction, provider fallback, UX thinking-block).

---

## 4. System Maturity Score

| Dimension | Score | Rationale |
|-----------|------:|-----------|
| Architecture | **8.5 / 10** | Clean layering (`fs_ops`, `tools`, `ai`, `controller`, `tasks`, `trace`, `memory`), well-named primitives, typed cancellation. Loses points for the placeholder `call_executor_with_fallback` and the implicit provider mode. |
| Execution reliability | **8.0 / 10** | Real cancel + tree-kill + timeouts + circuit breaker + retries + RAII guards. Loses points for no request-level retry and no context compaction. |
| Context grounding | **8.5 / 10** | Project scan + `project_context_summary` + executor/reviewer anti-hallucination prompts + real tool round-tripping. Loses points for not consulting `task_history` / `failures_log`. |
| UX quality | **5.5 / 10** | Four panes work, events render correctly, confirm modal is clean, task panel is behaviourally defensive. **But**: no thinking block, no summary-vs-reasoning separation, no collapse, noisy multi-bubble streams, button labels don't disambiguate turn-vs-goal cancel. |
| Production readiness | **6.5 / 10** | Packaging exists, settings persist to OS config dir, atomic memory writes, no lint/test CI, no telemetry, no update channel, single-user only. |

**Overall: 7.4 / 10.**

### Final verdict

> **Is this production-ready? Not yet — but close, and for the right reasons.**
>
> It is *production-capable* for a power user running locally on their own box with their own Ollama install. It is *not yet a shippable product* to a non-engineer audience, because (a) the provider routing is not explicit, (b) there is no Thinking-UI discipline, (c) there is no context compaction, and (d) there is no release-engineering spine (CI, update, telemetry).
>
> Closing those four items turns a strong research snapshot into a genuine Devin / Windsurf-class local AI workspace.

---

## 5. Documentation Updates

`PROJECT_MEMORY.json` and `PROJECT_PLAN.md` are unusually good for a snapshot — schema-versioned, kept updated per PR, no stale sections. Three concrete improvements:

1. **Add `ai_routing` block to `PROJECT_MEMORY.json → architecture`** describing the new `provider_mode` / `routing_strategy` / fallback matrix (see §6). Keeps docs aligned with the implementation this roadmap adds.
2. **Surface the `autonomous_confirm_irreversible` flag in `README.md` with a worked example** — today it's listed in `settings_surface` inside the JSON memory but the README walkthrough doesn't show a user turning it on for an unattended session.
3. **Add a "Known limits" section to README** (mirroring the Medium-severity items in §2.5): no context compaction, no cross-provider executor fallback, no retrieval over `task_history`. Users who hit these shouldn't feel blindsided.

Proposed new top-level JSON subtree (drop under `architecture`):

```json
"ai_routing": {
  "mode_source": "settings.provider_mode",
  "strategies": {
    "cloud":   { "planner": "openrouter", "executor": "openrouter", "reviewer": "openrouter" },
    "local":   { "planner": "ollama",     "executor": "ollama",     "reviewer": "ollama" },
    "hybrid":  { "planner": "openrouter", "executor": "ollama",     "reviewer": "openrouter",
                 "fallbacks": { "ollama_down": "openrouter",
                                "openrouter_down": "ollama",
                                "rate_limited": "ollama" } }
  },
  "request_retry": { "attempts": 2, "backoff_ms": 300, "jitter": true },
  "context_compaction": { "strategy": "summarise_oldest",
                          "trigger_tokens": 12000,
                          "keep_last_turns": 6 }
}
```

The proposed `PROJECT_MEMORY.json` delta is included verbatim so the "update docs in lockstep with implementation" rhythm the project already follows isn't broken. No existing keys are renamed — only additions.

---

## 6. AI Provider Architecture (Multi-Provider Routing)

### 6.1 Modes

| Mode | Planner | Executor | Reviewer | When to use |
|------|---------|----------|----------|-------------|
| **Cloud** | OpenRouter | OpenRouter | OpenRouter | Best reasoning, highest cost, requires network. |
| **Local** | Ollama | Ollama | Ollama (or disabled) | Offline, private, free, slower, lower quality. |
| **Hybrid (recommended)** | OpenRouter | Ollama | OpenRouter | Planner/Reviewer reasoning-heavy → cloud; Executor tool-heavy → local & cheap. |

### 6.2 Hybrid routing rules

```
role=planner   → prefer OpenRouter;   fallback Ollama(+planner_local_model) on 5xx/timeout/network.
role=executor  → prefer Ollama;       fallback OpenRouter(+cheap model) on model-not-loaded / ECONNREFUSED.
role=reviewer  → prefer OpenRouter;   fallback Ollama when no key or cloud degraded.
```

**Guardrails** (prevent unnecessary calls):
- One **health cache** per provider, TTL 30 s, seeded by `check_planner` / `check_executor` / `probe_ollama`. A role-level call checks the cache before dialling.
- **Negative caching** on hard failures (e.g. `model not found`) for 60 s — don't retry the same model against the same provider in a tight loop.
- **Short-circuit on provider-down**: if the `reachable` flag in the cache is false, skip straight to the fallback.

### 6.3 Config surface

Extend `Settings` (rust struct + TS type) with:

```jsonc
{
  "provider_mode": "hybrid",               // "cloud" | "local" | "hybrid"
  "routing_strategy": "smart",             // future: "cost_aware" | "latency_aware"
  "openrouter_api_key": "...",
  "openrouter_model": "openrouter/auto",
  "openrouter_planner_model": null,        // overrides for fine-grained control
  "openrouter_executor_model": null,
  "openrouter_reviewer_model": null,
  "ollama_base_url": "http://localhost:11434",
  "ollama_model": "deepseek-coder:6.7b",
  "ollama_planner_model": null,
  "ollama_reviewer_model": "llama3.2:1b",
  "request_retry_attempts": 2,
  "request_retry_backoff_ms": 300,
  "context_compaction_enabled": true,
  "context_compaction_trigger_tokens": 12000
}
```

Backwards compatibility: if `provider_mode` is missing on load, derive it — `openrouter_api_key` non-empty → `hybrid`, else `local`. (Same behaviour as today, just explicit.)

### 6.4 Failure handling

| Failure | Behaviour |
|---------|-----------|
| Ollama ECONNREFUSED | Fall back to OpenRouter if `provider_mode != local` and key present; else fail loud. |
| Ollama `model not found` | Try once with `:latest` suffix, else fall back to OpenRouter; else fail loud. |
| OpenRouter 401/403 | Never fall back (it's a config bug) — emit `ai:error` with actionable text and disable planner for session. |
| OpenRouter 429 | Exponential backoff (`request_retry_attempts`), then fall back to Ollama. |
| OpenRouter 5xx / timeout | Retry per `request_retry_*`, then fall back. |
| Both down | Emit `ai:error` once, mark step failed, preserve trace. **Never infinite-loop** — the `request_retry_attempts` cap is an absolute ceiling per role per turn. |

### 6.5 Cost & performance

- **Cache**: per-turn, cache the `project_context_summary` (already done) and the planner output (reused by executor and reviewer).
- **Model pairing defaults** that minimise cost without killing quality:
  - Planner: `anthropic/claude-3.5-sonnet` or `openrouter/auto` (cloud) / `qwen2.5:latest` (local)
  - Executor: `deepseek-coder:6.7b` (local) / `openai/gpt-4o-mini` (cloud)
  - Reviewer: `llama3.2:1b` (local) / `openai/gpt-4o-mini` (cloud)
- **Skip reviewer** when the executor's final message contains zero tool calls *and* the turn matched a simple-question heuristic (length < 200 chars, no code fences). Purely a cost/latency optimisation; the user can force-review.

---

## 7. UI / UX Transformation Plan

### 7.1 Thinking UI (core)

Today: every agent role's tokens stream into their own bubble, side by side with the final answer.

Target:

- A single **thinking block** renders *above* the final answer.
- While streaming, it shows the faded, low-opacity live reasoning.
- On `ai:done`, the block **auto-collapses** into a one-line summary ("Planned 4 steps, read 3 files, wrote 1").
- A `↓` chevron expands it back to the full reasoning on demand.
- The **final answer** is the most visually prominent element: normal foreground, left-rail accent, larger line-height.
- Errors keep a distinct, non-collapsible presentation.

### 7.2 Message hierarchy

```
┌────────────────────────────────────────────────────────┐
│ user:   (plain bubble, right-aligned)                  │
├────────────────────────────────────────────────────────┤
│ ▾ thinking · planner · executor · reviewer             │  ← collapsed by default after done
│     summary: "read App.tsx, wrote Chat.tsx, reviewed"  │
├────────────────────────────────────────────────────────┤
│ assistant (final answer, prominent)                    │
│   Markdown-rendered, code blocks, inline diff links    │
└────────────────────────────────────────────────────────┘
```

- **Agent actions** (tool calls) live in the Execution pane — the Chat pane no longer lists them inline.
- A subtle inline badge in the assistant bubble ("2 files changed") links the user to the Execution pane to see diffs.

### 7.3 Task panel redesign

- **Three status groups**, collapsed by default: Running, Queued, Completed.
- Status transitions animate (fade+slide) rather than re-rendering the list.
- **No synthetic labels** — if `task:update` arrives before `task:added`, render a `…` skeleton row instead of fabricating text (today's synthesised `"(task)"` placeholder is better than nothing, but a skeleton is more honest).
- Failures log collapses into an "N past failures" link that opens a modal.

### 7.4 Interaction model

- **Chat-first.** Goal & Tasks collapses into a small pill at the top of Chat when empty, expands to full pane only when a goal is running.
- **Inline progress**: an always-visible thin progress bar under the composer (tasks done / total) while a goal runs.
- **No noisy logs by default**: Execution pane starts filtered to `errors + step transitions` only; a toggle reveals tool-call details.

### 7.5 Visual style

- **Soft, calm palette**: foreground `#e7e9ee`, dim `#7b8394`, accent `#7aa2f7`, success `#9ece6a`, warn `#e0af68`, error `#f7768e`. Backgrounds `#1a1b26` / `#24283b` / `#2a2e3d`.
- **Typography**: Inter or system UI for chrome; JetBrains Mono / Fira Code for code + diffs; 1.55 line-height in chat for readability.
- **Motion**: 160 ms ease-out on collapse/expand, 80 ms on state chips. Respect `prefers-reduced-motion`.
- **Spacing**: 12 px gutter inside panes, 16 px vertical rhythm in chat, never less than 32 px between the composer and the last bubble.

---

## 8. UI Architecture

```
src/
  App.tsx                  ← top-level layout, route-like pane switching
  state/
    chatStore.ts           ← messages, streaming partials, thinking-collapse state
    taskStore.ts           ← tree, failures, circuit-tripped
    settingsStore.ts       ← provider_mode + model pairs
    health.ts              ← cached reachability per provider
  components/
    Chat/
      Chat.tsx             ← composer + list, no reasoning logic
      MessageBubble.tsx    ← role-aware rendering
      ThinkingBlock.tsx    ← faded stream, auto-collapse, summary generator
      Summary.tsx          ← derives 1-line summary from the turn's trace/steps
    Execution/
      Execution.tsx        ← filter chips, step timeline
      DiffBlock.tsx        ← virtualized at >500 lines
      ToolCallCard.tsx
    Tasks/
      TaskPanel.tsx
      TaskRow.tsx          ← skeleton + real
      FailureDrawer.tsx
    Settings/
      Settings.tsx         ← adds provider_mode picker + model-pair grid
    ConfirmCmd.tsx
  hooks/
    useBackendEvent.ts     ← typed subscription with cleanup
    useThinkingLifecycle.ts← store-driven, returns {streaming, collapsed, toggle}
```

### 8.1 State management

- Replace scattered `useState` with a small **`zustand` store** (or a `useReducer`+context pair if the team prefers zero deps). One store per concern (chat, tasks, settings, health).
- Subscriptions are **selector-based** (`useChatStore(s => s.messages)`) so only components that need the new value re-render.

### 8.2 Thinking-block lifecycle

```
idle ──(ai:token role=planner|reviewer)──► streaming_reasoning
                                                │
                                                ├──(more tokens)──► streaming_reasoning
                                                │
                                                └──(ai:done)────► collapsed(summary)
collapsed ──(user clicks ↓)────► expanded
expanded  ──(user clicks ▴ or new turn)────► collapsed
```

Stored per-turn, keyed by `turn_id` so scrolling back shows each turn's own collapse state.

### 8.3 Summary generation

Deterministic, client-side, no extra model call:

```
summary = join(
  steps.map(s =>
    s.role === "planner" ? `planned ${planBulletCount}` :
    s.role === "executor" ? `${toolCallsInStep} tool calls` :
    s.role === "reviewer" ? (verdict === "ok" ? "reviewed ok" : "reviewed fix")
  ), " · "
);
```

Fallback to `first_line(final_assistant)` when steps are missing.

### 8.4 Performance

- Virtualize the chat list (`react-virtuoso`) once messages > 100.
- Virtualize diff blocks > 500 lines.
- Throttle `ai:token` appends to animation-frame using a ref-accumulated buffer.
- Memoize `ThinkingBlock`'s rendered markdown; re-parse only when tokens actually change.
- Keep `Execution` events capped at 500 with a "show older" control, not an infinite list.

---

## 9. Implementation Roadmap

Each phase is mergeable independently and ships a visible win.

### Phase 1 — Core stability & execution fixes (1–2 days)

**What**
- Real request-level retry wrapper around `stream_openrouter` / `stream_ollama` (2 attempts, 300 ms backoff, jittered).
- Context compaction on executor messages: when estimated tokens > `context_compaction_trigger_tokens`, replace the oldest N turns with a planner-generated summary line.
- Recover Mutex poisoning instead of `unwrap()`.
- Rename `call_executor_with_fallback` → `call_executor` (no behaviour change) to stop lying in code.
- Add `#[cfg(test)]` coverage for the retry wrapper and the compaction trigger.

**Why** Removes the one misleading identifier, eliminates a whole class of "flaky network" failures, makes long sessions actually sustainable.

**Impact** High — fixes items H3, H4, H5 from §2.5 and the naming bug from H1.

### Phase 2 — AI routing system (2–3 days)

**What**
- Add `provider_mode` + per-role model overrides to `Settings` (with derived-default for backcompat).
- Implement the health-cache + negative-cache layer.
- Implement cross-provider fallback per the §6.4 table — behind `provider_mode == "hybrid"`.
- Settings UI: mode picker (Cloud / Local / Hybrid), per-role model pickers, a "Test providers" button that calls the existing `check_*` / `probe_ollama` commands.

**Why** Closes H1/H2 from §2.5. Matches the explicit multi-provider story the README promises.

**Impact** High — unlocks "this actually works offline and gracefully online".

### Phase 3 — Thinking UI system (2–3 days)

**What**
- `ThinkingBlock` component (streaming reasoning, auto-collapse, summary).
- Remove separate-bubble-per-role rendering from Chat; consolidate into `thinking` + `final` per turn.
- Summary generator from step events + trace.
- Respect `prefers-reduced-motion`.

**Why** Transforms the Chat pane from a debug log into a product.

**Impact** High — the single biggest UX win per hour of work.

### Phase 4 — Task panel redesign (1–2 days)

**What**
- Group by status, animated transitions, skeleton rows for races.
- Failure drawer (modal) instead of an always-visible list.
- Inline progress under the composer.
- Collapse the Task pane to a pill when no goal is running.

**Why** Makes the pane pleasant to look at during a 30-task goal; today it's a wall of text.

**Impact** Medium — matters only when goals are long-running, but that's the flagship workflow.

### Phase 5 — Polish & animations (1 day)

**What**
- Motion pass (160 ms / 80 ms easings on collapse, chips, bubbles).
- Typography & spacing refresh (see §7.5).
- Virtualize chat > 100 msgs, diff > 500 lines.
- Filter chips in Execution pane (errors-only / tool-calls / steps).
- Lint + typecheck CI on PR (GitHub Actions matrix: Bun + Cargo on Linux/Windows).

**Why** Puts the last 10% of professional polish on the system.

**Impact** Medium — compounds with Phases 3 + 4 to land the Devin/Windsurf-level feel.

---

## 10. Final Verdict

**Production-ready?** *Not yet as a consumer product; yes as an engineering tool for its authors.*

The engine underneath is honest — real tools, real cancel, real sandbox, real grounding. The two gaps that keep it from being shippable are (1) the routing layer pretending to be hybrid when it is not, and (2) a chat UI that still leaks the machinery of planning/executing/reviewing at the user.

**Close Phases 1 → 3 and you have a genuine product.** Everything else is scale & polish.

---

### Appendix A — File references

- Sandbox: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/fs_ops.rs" />
- Cancel: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/cancel.rs" />
- Tool runtime + gates + subprocess lifecycle: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/tools.rs" />
- Multi-agent loop: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/ai.rs" />
- Autonomous controller: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/controller.rs" />
- Project scanner: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/project_scan.rs" />
- Task tree + emit helpers: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/tasks.rs" />
- Bounded trace: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/trace.rs" />
- Atomic memory: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/memory.rs" />
- Settings: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/settings.rs" />
- Shell: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/frontend/src/App.tsx" />
- Chat: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/frontend/src/components/Chat.tsx" />
- Task panel: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/frontend/src/components/TaskPanel.tsx" />
- Execution pane: <ref_file file="/home/ubuntu/repos/open-claude-code-main/desktop/frontend/src/components/Execution.tsx" />
- Memory schema: <ref_file file="/home/ubuntu/repos/open-claude-code-main/PROJECT_MEMORY.json" />
- Plan: <ref_file file="/home/ubuntu/repos/open-claude-code-main/PROJECT_PLAN.md" />

### Appendix B — Selected fingerprints

- Misleading "fallback" helper: <ref_snippet file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/ai.rs" lines="573-584" />
- Write-gate path bypass fix: <ref_snippet file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/tools.rs" lines="290-330" />
- Reviewer retry & re-entry (only executor restarts): <ref_snippet file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/ai.rs" lines="977-1032" />
- Goal timeout trips both tokens: <ref_snippet file="/home/ubuntu/repos/open-claude-code-main/desktop/src-tauri/src/controller.rs" lines="185-216" />
- Multi-bubble per-role streaming (today's UX): <ref_snippet file="/home/ubuntu/repos/open-claude-code-main/desktop/frontend/src/components/Chat.tsx" lines="40-88" />
