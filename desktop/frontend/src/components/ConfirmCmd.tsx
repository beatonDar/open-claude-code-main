import { useEffect, useState } from "react";
import { api, onEvent } from "../api";
import type { ConfirmRequest, Settings } from "../types";

/**
 * Modal that resolves a pending `ai:confirm_request`. The backend refuses to
 * run non-allow-listed commands until the UI calls `api.confirmCmd`.
 *
 * "Allow once" → approves this one invocation.
 * "Allow & remember" → approves AND appends the command's first token to the
 *                      persistent allow-list so it runs silently next time.
 * "Deny" → rejects, executor sees `refused: user denied …`.
 */
export function ConfirmCmdOverlay() {
  const [pending, setPending] = useState<ConfirmRequest[]>([]);

  useEffect(() => {
    const p = onEvent<ConfirmRequest>("ai:confirm_request", (req) => {
      setPending((prev) => [...prev, req]);
    });
    return () => {
      void p.then((fn) => fn());
    };
  }, []);

  const resolve = async (
    id: string,
    approved: boolean,
    remember: boolean,
    cmd: string,
  ) => {
    if (approved && remember) {
      try {
        const s: Settings = await api.getSettings();
        const token = cmd.trim().split(/\s+/)[0] ?? "";
        if (token && !s.cmd_allow_list.includes(token)) {
          await api.saveSettings({
            ...s,
            cmd_allow_list: [...s.cmd_allow_list, token],
          });
        }
      } catch {
        // Non-fatal; still resolve the confirmation below.
      }
    }
    try {
      await api.confirmCmd(id, approved);
    } finally {
      setPending((prev) => prev.filter((r) => r.id !== id));
    }
  };

  if (pending.length === 0) return null;
  const req = pending[0];
  return (
    <div className="settings-overlay">
      <div className="settings-modal confirm-modal">
        <h2>Allow shell command?</h2>
        <p style={{ color: "var(--fg-dim)", fontSize: 12, marginBottom: 8 }}>
          The AI wants to run the following command inside{" "}
          <code style={{ fontFamily: "var(--mono)" }}>{req.project_dir}</code>.
        </p>
        <pre className="cmd-preview">{req.cmd}</pre>
        <div className="actions">
          <button onClick={() => void resolve(req.id, false, false, req.cmd)}>
            Deny
          </button>
          <button
            onClick={() => void resolve(req.id, true, true, req.cmd)}
            title="Approve and add the command's first token to the allow-list"
          >
            Allow &amp; remember
          </button>
          <button
            className="primary"
            onClick={() => void resolve(req.id, true, false, req.cmd)}
          >
            Allow once
          </button>
        </div>
      </div>
    </div>
  );
}
