//! Persistent project memory at `<project>/PROJECT_MEMORY.json`.
//!
//! This is intentionally just a JSON blob; callers hand us the full document
//! and we write it atomically (via temp-file + rename). The AI layer updates
//! `session.turns`, `file_index`, `tool_usage`, and `decisions` after every
//! chat turn.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::ai::UiToolCall;

const FILE: &str = "PROJECT_MEMORY.json";
/// Bound the session log so the memory file doesn't grow without limit.
const MAX_SESSION_TURNS: usize = 50;
const MAX_FILE_INDEX: usize = 500;
const MAX_DECISIONS: usize = 100;

fn memory_path(project_dir: &str) -> PathBuf {
    PathBuf::from(project_dir).join(FILE)
}

#[tauri::command]
pub fn load_memory(project_dir: String) -> Result<Value, String> {
    let path = memory_path(&project_dir);
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_memory(project_dir: String, memory: Value) -> Result<(), String> {
    save_memory_sync(&project_dir, &memory)
}

pub(crate) fn save_memory_sync(project_dir: &str, memory: &Value) -> Result<(), String> {
    let path = memory_path(project_dir);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(memory).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

fn unix_ts() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Merge the result of a chat turn into `PROJECT_MEMORY.json`. Creates the
/// file if it does not yet exist. Never fails catastrophically — on a
/// parse error we reset the affected subtree rather than refusing to write.
pub(crate) fn update_turn_memory(
    project_dir: &str,
    user_message: &str,
    assistant: &str,
    tool_calls: &[UiToolCall],
    touched_files: &[String],
    plan_text: Option<&str>,
) -> Result<(), String> {
    let path = memory_path(project_dir);
    let mut mem: Value = match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    };
    if !mem.is_object() {
        mem = json!({});
    }
    let obj = mem.as_object_mut().unwrap();

    let now = unix_ts();
    obj.insert("updated_at".into(), Value::String(format!("epoch:{now}")));

    // ---------- session.turns ----------
    let session = obj
        .entry("session".to_string())
        .or_insert_with(|| json!({ "turns": [], "opened_project": null }));
    if !session.is_object() {
        *session = json!({ "turns": [] });
    }
    let sobj = session.as_object_mut().unwrap();
    sobj.insert(
        "opened_project".into(),
        Value::String(project_dir.to_string()),
    );
    let turns = sobj
        .entry("turns".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(arr) = turns.as_array_mut() {
        arr.push(json!({
            "at": now,
            "user": user_message,
            "assistant": assistant,
            "tool_calls": tool_calls.iter().map(|t| &t.name).collect::<Vec<_>>(),
            "touched_files": touched_files,
        }));
        let overflow = arr.len().saturating_sub(MAX_SESSION_TURNS);
        if overflow > 0 {
            arr.drain(..overflow);
        }
    }

    // ---------- tool_usage ----------
    let tu = obj
        .entry("tool_usage".to_string())
        .or_insert_with(|| json!({}));
    if let Some(tu_obj) = tu.as_object_mut() {
        for tc in tool_calls {
            let entry = tu_obj.entry(tc.name.clone()).or_insert(Value::from(0u64));
            if let Some(n) = entry.as_u64() {
                *entry = Value::from(n + 1);
            } else {
                *entry = Value::from(1u64);
            }
        }
    }

    // ---------- file_index ----------
    //
    // Track every file the executor touched, with a rolling count and
    // last-access timestamp. Bounded so the memory file stays small.
    let fi = obj
        .entry("file_index".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !fi.is_array() {
        *fi = Value::Array(Vec::new());
    }
    let fi_arr = fi.as_array_mut().unwrap();
    for path in touched_files {
        if path.is_empty() {
            continue;
        }
        let existing_idx = fi_arr.iter().position(|entry| {
            entry
                .get("path")
                .and_then(|p| p.as_str())
                .map(|p| p == path)
                .unwrap_or(false)
        });
        match existing_idx {
            Some(i) => {
                let entry = &mut fi_arr[i];
                if let Some(o) = entry.as_object_mut() {
                    o.insert("last_accessed".into(), Value::from(now));
                    let count = o
                        .get("count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                        + 1;
                    o.insert("count".into(), Value::from(count));
                }
            }
            None => {
                fi_arr.push(json!({
                    "path": path,
                    "first_seen": now,
                    "last_accessed": now,
                    "count": 1,
                }));
            }
        }
    }
    // Evict oldest by last_accessed.
    if fi_arr.len() > MAX_FILE_INDEX {
        fi_arr.sort_by_key(|entry| {
            entry
                .get("last_accessed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        });
        let drop = fi_arr.len() - MAX_FILE_INDEX;
        fi_arr.drain(..drop);
    }

    // ---------- decisions (planner notes) ----------
    if let Some(plan) = plan_text {
        if !plan.trim().is_empty() {
            let decisions = obj
                .entry("decisions".to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            if !decisions.is_array() {
                *decisions = Value::Array(Vec::new());
            }
            if let Some(arr) = decisions.as_array_mut() {
                arr.push(json!({
                    "at": now,
                    "role": "planner",
                    "user": user_message,
                    "text": plan.trim(),
                }));
                let overflow = arr.len().saturating_sub(MAX_DECISIONS);
                if overflow > 0 {
                    arr.drain(..overflow);
                }
            }
        }
    }

    save_memory_sync(project_dir, &mem)
}
