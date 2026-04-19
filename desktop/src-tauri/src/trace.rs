//! Per-task execution trace.
//!
//! Every task executed by the autonomous controller is wrapped with a
//! bounded `TaskTrace` that captures the full agent transcript — the
//! user's instruction, planner output, each executor message, every tool
//! call + result pair, reviewer verdicts, retry markers, and errors.
//!
//! Traces are attached to `Task` (via `Task.trace`) and persisted with
//! the rest of the tree into `active_task_tree` and `task_history`, so
//! you can reconstruct *why* an agent did what it did by reading a
//! single JSON blob per task.
//!
//! Size is bounded both per-field (`MAX_TEXT_CHARS`) and per-trace
//! (`MAX_ENTRIES`). When the entry cap is hit we drop the *oldest*
//! entries (keeping the initial `User`/`Plan` markers for context) and
//! flip `truncated = true` so the UI can surface it. Large tool outputs
//! and diffs are truncated individually so a single 10 MB file read
//! can't blow the rest of the trace out of the cap.

use serde::{Deserialize, Serialize};

/// Hard cap on entries per task. A normal multi-tool task is ~5–30
/// entries; we leave plenty of headroom for long executor loops and
/// reviewer retries before truncation kicks in.
pub const MAX_ENTRIES: usize = 200;

/// Per-field text cap. Matched to 4 KiB so a truncated blob still gives
/// useful context (full stack trace, meaningful diff hunk) but one
/// malicious file-read can't fill 10 MB of memory per entry.
pub const MAX_TEXT_CHARS: usize = 4_096;

/// A single trace record. Tagged by `kind` so the TS frontend can narrow
/// on it directly. `at` is unix seconds; each entry has one so UIs can
/// render a timeline without needing `updated_at` bookkeeping.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceEntry {
    /// The user instruction that kicked off this task.
    User { text: String, at: u64 },
    /// Planner output for this task, if the planner ran.
    Plan { text: String, at: u64 },
    /// Assistant message from one of the agent roles.
    Assistant {
        role: String,
        text: String,
        at: u64,
    },
    /// A tool call issued by an agent.
    ToolCall {
        id: String,
        role: String,
        name: String,
        args: String,
        at: u64,
    },
    /// The result of a tool call. `id` matches the preceding `ToolCall`.
    ToolResult {
        id: String,
        role: String,
        ok: bool,
        output: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
        at: u64,
    },
    /// Reviewer verdict (`ok`/`needs_fix`/`unknown`) plus the full text.
    Review {
        verdict: String,
        text: String,
        at: u64,
    },
    /// Marks a retry boundary inside the trace so it's visible where the
    /// executor restarted from reviewer feedback or a transient error.
    Retry {
        attempt: u32,
        reason: String,
        at: u64,
    },
    /// A surfaced error from any agent role. Distinct from a failed
    /// `ToolResult` (`ok: false`) because it covers *non*-tool errors —
    /// planner timeout, SSE failure, model 5xx, etc.
    Error {
        role: String,
        message: String,
        at: u64,
    },
}

/// Bounded transcript for a single task. Crossing `MAX_ENTRIES` drops
/// the oldest non-`User` entry first so the user's instruction (the
/// first entry by convention) stays pinned to the top.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskTrace {
    #[serde(default)]
    pub entries: Vec<TraceEntry>,
    #[serde(default)]
    pub truncated: bool,
}

impl TaskTrace {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Push a new entry, enforcing the length cap. On overflow we drop
    /// the oldest *non-initial* entry so the user instruction at index
    /// 0 is preserved for context.
    pub fn push(&mut self, entry: TraceEntry) {
        if self.entries.len() >= MAX_ENTRIES {
            // Keep entry 0 pinned; drop index 1 (oldest after pinned).
            // If the pinned entry is absent (push called out of order),
            // fall back to dropping the oldest.
            let drop_idx = if self.entries.len() > 1 { 1 } else { 0 };
            self.entries.remove(drop_idx);
            self.truncated = true;
        }
        self.entries.push(entry);
    }

    pub fn push_user(&mut self, text: &str, at: u64) {
        self.push(TraceEntry::User {
            text: cap(text),
            at,
        });
    }

    pub fn push_plan(&mut self, text: &str, at: u64) {
        self.push(TraceEntry::Plan {
            text: cap(text),
            at,
        });
    }

    pub fn push_assistant(&mut self, role: &str, text: &str, at: u64) {
        self.push(TraceEntry::Assistant {
            role: role.into(),
            text: cap(text),
            at,
        });
    }

    pub fn push_tool_call(&mut self, id: &str, role: &str, name: &str, args: &str, at: u64) {
        self.push(TraceEntry::ToolCall {
            id: id.into(),
            role: role.into(),
            name: name.into(),
            args: cap(args),
            at,
        });
    }

    pub fn push_tool_result(
        &mut self,
        id: &str,
        role: &str,
        ok: bool,
        output: &str,
        diff: Option<&str>,
        at: u64,
    ) {
        self.push(TraceEntry::ToolResult {
            id: id.into(),
            role: role.into(),
            ok,
            output: cap(output),
            diff: diff.map(cap),
            at,
        });
    }

    pub fn push_review(&mut self, verdict: &str, text: &str, at: u64) {
        self.push(TraceEntry::Review {
            verdict: verdict.into(),
            text: cap(text),
            at,
        });
    }

    pub fn push_retry(&mut self, attempt: u32, reason: &str, at: u64) {
        self.push(TraceEntry::Retry {
            attempt,
            reason: cap(reason),
            at,
        });
    }

    pub fn push_error(&mut self, role: &str, message: &str, at: u64) {
        self.push(TraceEntry::Error {
            role: role.into(),
            message: cap(message),
            at,
        });
    }
}

/// Truncate to `MAX_TEXT_CHARS` characters (not bytes) and append `…` if
/// shortened, preserving UTF-8 boundaries.
fn cap(s: &str) -> String {
    if s.chars().count() <= MAX_TEXT_CHARS {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(MAX_TEXT_CHARS).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_trace() -> TaskTrace {
        let mut t = TaskTrace::new();
        t.push_user("do the thing", 10);
        t
    }

    #[test]
    fn empty_trace_is_empty() {
        let t = TaskTrace::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert!(!t.truncated);
    }

    #[test]
    fn push_tool_call_and_result_round_trip_json() {
        let mut t = mk_trace();
        t.push_tool_call("call_1", "executor", "run_cmd", r#"{"cmd":"ls"}"#, 11);
        t.push_tool_result("call_1", "executor", true, "a\nb\n", Some("+a\n"), 12);

        let j = serde_json::to_value(&t).unwrap();
        let entries = j.get("entries").unwrap().as_array().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1]["kind"], "tool_call");
        assert_eq!(entries[1]["name"], "run_cmd");
        assert_eq!(entries[2]["kind"], "tool_result");
        assert_eq!(entries[2]["ok"], true);
        assert_eq!(entries[2]["diff"], "+a\n");
    }

    #[test]
    fn cap_truncates_long_text_and_appends_ellipsis() {
        let mut t = TaskTrace::new();
        let huge = "x".repeat(MAX_TEXT_CHARS + 50);
        t.push_user(&huge, 0);
        let j = serde_json::to_value(&t).unwrap();
        let text = j["entries"][0]["text"].as_str().unwrap();
        // `…` is a single char but 3 bytes, so char count is MAX+1.
        assert_eq!(text.chars().count(), MAX_TEXT_CHARS + 1);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn cap_preserves_utf8_boundary() {
        let mut t = TaskTrace::new();
        let s: String = "🦀".repeat(MAX_TEXT_CHARS + 10);
        t.push_user(&s, 0);
        let j = serde_json::to_value(&t).unwrap();
        let out = j["entries"][0]["text"].as_str().unwrap();
        assert_eq!(out.chars().count(), MAX_TEXT_CHARS + 1);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn overflow_drops_oldest_non_initial_and_sets_truncated() {
        let mut t = TaskTrace::new();
        // Entry 0 = pinned user.
        t.push_user("pinned", 0);
        // Fill to the cap with assistant entries tagged by timestamp.
        for i in 1..MAX_ENTRIES as u64 {
            t.push_assistant("executor", &format!("step {i}"), i);
        }
        assert_eq!(t.len(), MAX_ENTRIES);
        assert!(!t.truncated);

        // One more push should evict index 1 (oldest non-pinned).
        t.push_assistant("executor", "overflow", 9_999);
        assert_eq!(t.len(), MAX_ENTRIES);
        assert!(t.truncated);
        // Pinned user still at index 0.
        let j = serde_json::to_value(&t).unwrap();
        let entries = j["entries"].as_array().unwrap();
        assert_eq!(entries[0]["kind"], "user");
        assert_eq!(entries[0]["text"], "pinned");
        // The new entry landed at the tail.
        assert_eq!(entries.last().unwrap()["text"], "overflow");
        // Entry 1 is no longer the "step 1" we started with.
        assert_ne!(entries[1]["text"], "step 1");
    }

    #[test]
    fn retry_and_error_entries_carry_their_fields() {
        let mut t = TaskTrace::new();
        t.push_retry(2, "reviewer NEEDS_FIX: restart", 50);
        t.push_error("planner", "SSE 503 from OpenRouter", 51);
        let j = serde_json::to_value(&t).unwrap();
        let entries = j["entries"].as_array().unwrap();
        assert_eq!(entries[0]["kind"], "retry");
        assert_eq!(entries[0]["attempt"], 2);
        assert_eq!(entries[1]["kind"], "error");
        assert_eq!(entries[1]["role"], "planner");
    }

    #[test]
    fn review_verdict_round_trip() {
        let mut t = TaskTrace::new();
        t.push_review("needs_fix", "The function is missing docstrings.", 60);
        let j = serde_json::to_value(&t).unwrap();
        assert_eq!(j["entries"][0]["kind"], "review");
        assert_eq!(j["entries"][0]["verdict"], "needs_fix");
    }

    #[test]
    fn deserialize_from_persisted_json_is_lossless_for_known_kinds() {
        // Lock in the on-disk contract: if a trace is written as JSON
        // and then read back, every known variant round-trips.
        let mut t = TaskTrace::new();
        t.push_user("u", 1);
        t.push_plan("p", 2);
        t.push_assistant("executor", "a", 3);
        t.push_tool_call("c1", "executor", "read_file", "{}", 4);
        t.push_tool_result("c1", "executor", true, "ok", Some("d"), 5);
        t.push_review("ok", "looks fine", 6);
        t.push_retry(1, "flaky", 7);
        t.push_error("reviewer", "boom", 8);

        let raw = serde_json::to_string(&t).unwrap();
        let round: TaskTrace = serde_json::from_str(&raw).unwrap();
        assert_eq!(round.entries.len(), t.entries.len());
        assert_eq!(round.truncated, t.truncated);
        // Shape parity via another round-trip.
        let raw2 = serde_json::to_string(&round).unwrap();
        assert_eq!(raw, raw2);
    }

    #[test]
    fn missing_trace_deserializes_to_default() {
        // Tasks persisted before this PR have no `trace` field. Make
        // sure we never break on loading them.
        let v: TaskTrace = serde_json::from_str("{}").unwrap();
        assert!(v.is_empty());
        assert!(!v.truncated);
    }
}
