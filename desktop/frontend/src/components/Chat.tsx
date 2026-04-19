import { useCallback, useEffect, useRef, useState } from "react";
import { api, onEvent, type DoneEvent, type TokenEvent, type ErrorEvent } from "../api";
import type { AgentRole, ChatMessage } from "../types";

type Props = {
  projectDir: string | null;
  messages: ChatMessage[];
  setMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  disabled: boolean;
};

function uid() {
  return Math.random().toString(36).slice(2, 10);
}

function roleColor(r?: AgentRole): string {
  switch (r) {
    case "planner":
      return "role-planner";
    case "executor":
      return "role-executor";
    case "reviewer":
      return "role-reviewer";
    default:
      return "";
  }
}

export function Chat({ projectDir, messages, setMessages, disabled }: Props) {
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages]);

  // Stream tokens into the chat. Each agent-role run gets its own bubble so
  // the user can see Planner → Executor → Reviewer unfold in order.
  useEffect(() => {
    const unlistens: Array<Promise<() => void>> = [];
    unlistens.push(
      onEvent<TokenEvent>("ai:token", (p) => {
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last && last.streaming && last.streaming_role === p.role) {
            return [
              ...prev.slice(0, -1),
              { ...last, content: last.content + p.text },
            ];
          }
          return [
            ...prev,
            {
              id: uid(),
              role: "assistant",
              content: p.text,
              streaming: true,
              streaming_role: p.role,
            },
          ];
        });
      }),
    );
    unlistens.push(
      onEvent<DoneEvent>("ai:done", () => {
        setMessages((prev) =>
          prev.map((m) => (m.streaming ? { ...m, streaming: false } : m)),
        );
      }),
    );
    unlistens.push(
      onEvent<ErrorEvent>("ai:error", (p) => {
        setMessages((prev) => [
          ...prev,
          {
            id: uid(),
            role: "system",
            content: `[${p.role ?? "ai"}] error: ${p.message}`,
          },
        ]);
      }),
    );
    return () => {
      for (const p of unlistens) void p.then((fn) => fn());
    };
  }, [setMessages]);

  const send = useCallback(async () => {
    if (!projectDir || sending || input.trim().length === 0) return;
    const userMsg: ChatMessage = { id: uid(), role: "user", content: input.trim() };
    const historyForCall = messages.filter((m) => !m.streaming);
    setMessages([...messages, userMsg]);
    setInput("");
    setSending(true);
    try {
      const resp = await api.sendChat(projectDir, userMsg.content, historyForCall);
      setMessages((prev) => {
        // Clear any lingering streaming flags from dropped frames.
        const cleared = prev.map((m) =>
          m.streaming ? { ...m, streaming: false } : m,
        );
        // If streaming never produced an executor bubble, synthesize one from
        // the final response so the user sees *something*.
        const hasExecutorBubble = cleared.some(
          (m) =>
            m.role === "assistant" &&
            (m.streaming_role === "executor" || !m.streaming_role) &&
            m.content.length > 0,
        );
        if (!hasExecutorBubble && resp.assistant) {
          return [
            ...cleared,
            {
              id: uid(),
              role: "assistant",
              content: resp.assistant,
              streaming_role: "executor",
              tool_calls: resp.tool_calls,
              tool_results: resp.tool_results,
            },
          ];
        }
        // Otherwise attach tool metadata to the last executor bubble.
        const lastExecutorIdx = [...cleared]
          .map((m, i) => ({ m, i }))
          .reverse()
          .find(
            ({ m }) =>
              m.role === "assistant" &&
              (m.streaming_role === "executor" || !m.streaming_role),
          )?.i;
        if (lastExecutorIdx != null) {
          const next = [...cleared];
          next[lastExecutorIdx] = {
            ...next[lastExecutorIdx],
            tool_calls: resp.tool_calls,
            tool_results: resp.tool_results,
          };
          return next;
        }
        return cleared;
      });
    } catch (e) {
      setMessages((prev) => [
        ...prev.map((m) => (m.streaming ? { ...m, streaming: false } : m)),
        {
          id: uid(),
          role: "assistant",
          content: `Error: ${String(e)}`,
        },
      ]);
    } finally {
      setSending(false);
    }
  }, [input, messages, projectDir, sending, setMessages]);

  return (
    <div className="chat">
      <div className="chat-messages" ref={scrollRef}>
        {messages.length === 0 && (
          <div className="empty-state">
            Ask the AI to read, edit, or run commands in your project.
          </div>
        )}
        {messages.map((m) => (
          <div
            key={m.id}
            className={
              "msg role-" + m.role + " " + roleColor(m.streaming_role)
            }
          >
            <div className="msg-role">
              {m.role}
              {m.streaming_role ? (
                <span className={"role-chip chip-" + m.streaming_role}>
                  {m.streaming_role}
                </span>
              ) : null}
              {m.streaming ? (
                <span className="streaming-dot" aria-label="streaming" />
              ) : null}
            </div>
            <div>{m.content || (m.streaming ? "…" : "")}</div>
            {m.tool_calls && m.tool_calls.length > 0 && (
              <div className="tool-summary">
                {m.tool_calls.length} tool call
                {m.tool_calls.length > 1 ? "s" : ""}:&nbsp;
                {m.tool_calls.map((t) => t.name).join(", ")}
              </div>
            )}
          </div>
        ))}
        {sending && !messages.some((m) => m.streaming) && (
          <div className="msg role-assistant">
            <div className="msg-role">assistant</div>
            <div style={{ color: "var(--fg-dim)" }}>thinking…</div>
          </div>
        )}
      </div>
      <div className="composer">
        <textarea
          placeholder={
            disabled
              ? "Open a project and ensure Ollama is reachable…"
              : "Ask the AI… (Ctrl+Enter to send)"
          }
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              void send();
            }
          }}
          disabled={disabled || sending}
        />
        {sending ? (
          <button className="danger" onClick={() => void api.cancelChat()}>
            Stop
          </button>
        ) : (
          <button
            className="primary"
            disabled={disabled || input.trim().length === 0}
            onClick={() => void send()}
          >
            Send
          </button>
        )}
      </div>
    </div>
  );
}
