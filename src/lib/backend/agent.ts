// The agent's side of the backend: model providers, asking a model, and the tools that act on a
// server. API keys are sent in once and never come back.

import { invoke } from "@tauri-apps/api/core";

export type ProviderKind = "anthropic" | "open_ai_compatible";

export interface Provider {
  /** Empty for a new one. */
  id: string;
  name: string;
  kind: ProviderKind;
  base_url: string;
  model: string;
}

export interface ProviderView extends Provider {
  has_key: boolean;
  /** The key is only kept until the app closes (the system keychain isn't available). */
  key_for_now_only: boolean;
}

export interface ToolCall {
  id: string;
  name: string;
  input: unknown;
}

export interface ToolResult {
  call_id: string;
  content: string;
  is_error: boolean;
}

export type Message =
  | { role: "user"; text: string }
  | { role: "assistant"; text: string; tool_calls: ToolCall[] }
  | { role: "tool_results"; results: ToolResult[] };

export interface ToolDef {
  name: string;
  description: string;
  /** JSON Schema of the input. */
  schema: unknown;
}

export interface Reply {
  text: string;
  tool_calls: ToolCall[];
  stop: "done" | "tools" | "length";
  input_tokens: number;
  output_tokens: number;
}

export type Risk = "read_only" | "changes" | "blocked" | "dangerous";

/** How the agent behaves. Empty text means the built-in one. */
export interface AgentSettings {
  system_prompt: string;
  extra_instructions: string;
  max_rounds: number;
  token_cap: number;
  /** Lets normally blocked commands be proposed (each still needs approval). */
  allow_dangerous: boolean;
}

export function agentSettingsGet(): Promise<AgentSettings> {
  return invoke<AgentSettings>("agent_settings_get");
}

export function agentSettingsSet(settings: AgentSettings): Promise<void> {
  return invoke<void>("agent_settings_set", { settings });
}

export function agentProviders(): Promise<ProviderView[]> {
  return invoke<ProviderView[]>("agent_providers");
}

/** Saves a provider. An empty or missing key keeps the one already stored. */
export function agentProviderSave(provider: Provider, apiKey?: string): Promise<ProviderView> {
  return invoke<ProviderView>("agent_provider_save", { provider, apiKey: apiKey ?? null });
}

export function agentProviderDelete(id: string): Promise<void> {
  return invoke<void>("agent_provider_delete", { id });
}

export function agentComplete(
  requestId: string,
  providerId: string,
  system: string,
  messages: Message[],
  tools: ToolDef[],
): Promise<Reply> {
  return invoke<Reply>("agent_complete", { requestId, providerId, system, messages, tools });
}

export function agentCancel(requestId: string): Promise<void> {
  return invoke<void>("agent_cancel", { requestId });
}

export function agentClassify(command: string): Promise<Risk> {
  return invoke<Risk>("agent_classify", { command });
}

export function agentRedact(text: string): Promise<string> {
  return invoke<string>("agent_redact", { text });
}

export function agentExec(
  savedSessionId: string,
  command: string,
  approved: boolean,
): Promise<{ code: number | null; output: string }> {
  return invoke("agent_exec", { savedSessionId, command, approved });
}

export function agentReadFile(filesId: string, path: string, approved: boolean): Promise<string> {
  return invoke<string>("agent_read_file", { filesId, path, approved });
}

export function agentWriteFile(filesId: string, path: string, content: string, approved: boolean): Promise<void> {
  return invoke<void>("agent_write_file", { filesId, path, content, approved });
}
