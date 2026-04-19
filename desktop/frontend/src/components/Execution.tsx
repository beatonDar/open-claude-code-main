import { useMemo } from "react";
import type { AgentRole, ExecutionEvent, StepEvent } from "../types";

function fmtTime(ms: number) {
  const d = new Date(ms);
  return d.toLocaleTimeString();
}

function roleBadge(role?: AgentRole) {
  if (!role) return null;
  return <span className={"role-chip chip-" + role}>{role}</span>;
}

function DiffBlock({ diff }: { diff: string }) {
  const lines = diff.split("\n");
  return (
    <pre className="diff">
      {lines.map((line, i) => {
        let cls = "diff-ctx";
        if (line.startsWith("+++") || line.startsWith("---")) cls = "diff-meta";
        else if (line.startsWith("@@")) cls = "diff-hunk";
        else if (line.startsWith("+")) cls = "diff-add";
        else if (line.startsWith("-")) cls = "diff-del";
        return (
          <span key={i} className={cls}>
            {line}
            {"\n"}
          </span>
        );
      })}
    </pre>
  );
}

function StepRow({ step }: { step: StepEvent }) {
  const status =
    step.status === "running" ? "⋯" : step.status === "done" ? "✓" : "✗";
  return (
    <div className={"step-row status-" + step.status}>
      <span className="step-status" aria-hidden>
        {status}
      </span>
      {roleBadge(step.role)}
      <span className="step-title">{step.title}</span>
    </div>
  );
}

export function Execution({ events }: { events: ExecutionEvent[] }) {
  // Collapse step events into the latest version per index so the timeline
  // reflects each step's current state (running → done/failed).
  const steps = useMemo(() => {
    const byIndex = new Map<number, StepEvent>();
    for (const e of events) {
      if (e.kind === "step") byIndex.set(e.step.index, e.step);
    }
    return [...byIndex.values()].sort((a, b) => a.index - b.index);
  }, [events]);

  if (events.length === 0) {
    return (
      <div className="empty-state">
        Tool calls, command output, and file changes will appear here.
      </div>
    );
  }
  return (
    <div className="exec-list">
      {steps.length > 0 && (
        <div className="step-timeline">
          <div className="step-timeline-title">agent timeline</div>
          {steps.map((s) => (
            <StepRow key={s.index} step={s} />
          ))}
        </div>
      )}
      {events.map((e, i) => {
        if (e.kind === "step") {
          // Shown in the condensed timeline above; skip inline.
          return null;
        }
        if (e.kind === "tool_call") {
          return (
            <div key={i} className="exec-item call">
              <div className="title">
                {fmtTime(e.at)} → tool_call: <strong>{e.call.name}</strong>
                {roleBadge(e.call.role)}
              </div>
              <pre>{JSON.stringify(e.call.args, null, 2)}</pre>
            </div>
          );
        }
        if (e.kind === "tool_result") {
          return (
            <div
              key={i}
              className={
                "exec-item result " + (e.result.ok ? "ok" : "err")
              }
            >
              <div className="title">
                {fmtTime(e.at)} ← tool_result {e.result.ok ? "✓" : "✗"} (id{" "}
                {e.result.id.slice(0, 6)}){roleBadge(e.result.role)}
              </div>
              {e.result.diff ? (
                <DiffBlock diff={e.result.diff} />
              ) : (
                <pre>{e.result.output || "(empty)"}</pre>
              )}
            </div>
          );
        }
        if (e.kind === "error") {
          return (
            <div key={i} className="exec-item error">
              <div className="title">
                {fmtTime(e.at)} — error{roleBadge(e.role)}
              </div>
              <pre>{e.text}</pre>
            </div>
          );
        }
        return (
          <div key={i} className="exec-item info">
            <div className="title">{fmtTime(e.at)} — info</div>
            <pre>{e.text}</pre>
          </div>
        );
      })}
    </div>
  );
}
