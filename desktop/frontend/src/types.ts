export type FsEntry = {
  name: string;
  path: string;
  is_dir: boolean;
  size: number | null;
};

export type FsChange = {
  path: string;
  kind: "created" | "modified" | "removed" | "renamed" | "other";
};

export type AgentRole = "planner" | "executor" | "reviewer";

export type ToolCall = {
  id: string;
  name: string;
  args: unknown;
  /** Which agent issued this tool call. */
  role?: AgentRole;
};

export type ToolResult = {
  id: string;
  ok: boolean;
  output: string;
  diff: string | null;
  role?: AgentRole;
};

export type ChatMessage = {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  tool_calls?: ToolCall[];
  tool_results?: ToolResult[];
  streaming?: boolean;
  /** Which agent authored the streaming partial, if any. */
  streaming_role?: AgentRole;
};

export type Settings = {
  openrouter_api_key: string;
  openrouter_model: string;
  ollama_base_url: string;
  ollama_model: string;
  reviewer_enabled: boolean;
  max_iterations: number;
  cmd_confirm_required: boolean;
  cmd_allow_list: string[];
};

export type StepStatus = "running" | "done" | "failed";

export type StepEvent = {
  index: number;
  role: AgentRole;
  title: string;
  status: StepStatus;
};

export type ConfirmRequest = {
  id: string;
  cmd: string;
  project_dir: string;
  timeout_ms: number;
};

export type ExecutionEvent =
  | { kind: "tool_call"; call: ToolCall; at: number }
  | { kind: "tool_result"; result: ToolResult; at: number }
  | { kind: "step"; step: StepEvent; at: number }
  | { kind: "info"; text: string; at: number }
  | { kind: "error"; text: string; role?: AgentRole; at: number };
