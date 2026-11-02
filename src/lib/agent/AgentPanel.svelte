<script lang="ts">
  import { onMount, tick } from "svelte";
  import { agentProviders, type ProviderView } from "../backend/agent";
  import Icon from "../ui/Icon.svelte";
  import type { AgentChat, Context, ToolStep } from "./agent.svelte";
  import AgentSettings from "./AgentSettings.svelte";

  interface Props {
    chat: AgentChat;
    /** Where the agent is working: the active tab. */
    context: Context;
    onclose: () => void;
  }

  let { chat, context, onclose }: Props = $props();

  let providers = $state<ProviderView[]>([]);
  let showSettings = $state(false);
  let draft = $state("");
  let scroller: HTMLDivElement | undefined = $state();

  onMount(async () => {
    providers = await agentProviders().catch(() => []);
    if (!providers.some((p) => p.id === chat.providerId)) chat.setProvider(providers[0]?.id ?? "");
  });

  const provider = $derived(providers.find((p) => p.id === chat.providerId));
  const destination = $derived.by(() => {
    if (!provider) return "";
    try {
      return new URL(provider.base_url).host;
    } catch {
      return "";
    }
  });
  const local = $derived(destination.startsWith("localhost") || destination.startsWith("127.0.0.1"));

  function changed(list: ProviderView[]) {
    providers = list;
    if (!list.some((p) => p.id === chat.providerId)) chat.setProvider(list[0]?.id ?? "");
  }

  function submit() {
    const text = draft;
    if (!text.trim() || chat.running) return;
    draft = "";
    void chat.send(text, () => context);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  // Keep the newest step in view.
  $effect(() => {
    chat.steps.length;
    chat.steps.at(-1);
    void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  const riskLabel: Record<ToolStep["risk"], string> = {
    info: "Looks only",
    read_only: "Looks only",
    changes: "Changes things",
    blocked: "Blocked",
  };
  const stateLabel: Record<ToolStep["state"], string> = {
    waiting: "Waiting for you",
    running: "Running…",
    done: "Done",
    denied: "Not run",
    failed: "Failed",
  };
</script>

<aside class="panel" aria-label="Agent">
  <header>
    <span class="heading"><Icon name="sparkle" size={15} /> Agent</span>
    <span class="head-tools">
      <button class="icon-btn" title="Start a new conversation" aria-label="New conversation" onclick={() => chat.clear()}>
        <Icon name="refresh" size={15} />
      </button>
      <button class="icon-btn" title="Providers and keys" aria-label="Providers" onclick={() => (showSettings = true)}>
        <Icon name="settings" size={15} />
      </button>
      <button class="icon-btn" title="Close" aria-label="Close the agent" onclick={onclose}>
        <Icon name="close" size={15} />
      </button>
    </span>
  </header>

  <div class="setup">
    {#if providers.length}
      <select value={chat.providerId} onchange={(e) => chat.setProvider(e.currentTarget.value)} aria-label="Model provider">
        {#each providers as p (p.id)}
          <option value={p.id}>{p.name} · {p.model}</option>
        {/each}
      </select>
      {#if provider && !provider.has_key && !local}
        <p class="message warn">No API key saved for this provider. <button class="link" onclick={() => (showSettings = true)}>Add it</button></p>
      {:else if destination}
        <p class="privacy">{local ? "Runs on this computer." : `What it reads is sent to ${destination}, with passwords and keys removed.`} Nothing is saved to disk.</p>
      {/if}
    {:else}
      <p class="message warn">Choose where the agent's model comes from. <button class="btn small" onclick={() => (showSettings = true)}>Set up a provider</button></p>
    {/if}
    <p class="where">
      {#if context.savedSessionId}Working on <strong>{context.sessionName}</strong> (SSH){:else}No SSH tab is active, so it can only answer questions.{/if}
    </p>
  </div>

  <div class="log" bind:this={scroller}>
    {#if chat.steps.length === 0}
      <div class="empty">
        <p>Ask for help with the server in the active tab. It can look around, read files and the terminal, and — with your approval — run commands and edit files.</p>
        <p class="examples">Try: “why is nginx not starting?”, “what is using the disk?”, “read the terminal and tell me what the error means”.</p>
      </div>
    {/if}
    {#each chat.steps as step, i (i)}
      {#if step.kind === "user"}
        <div class="bubble user">{step.text}</div>
      {:else if step.kind === "assistant"}
        <div class="bubble assistant">{step.text}</div>
      {:else if step.kind === "note"}
        <p class="note-line">{step.text}</p>
      {:else}
        <div class="tool" class:waiting={step.state === "waiting"} class:bad={step.risk === "blocked" || step.state === "failed"}>
          <div class="tool-head">
            <strong>{step.title}</strong>
            <span class="badge {step.risk}">{riskLabel[step.risk]}</span>
            <span class="state">{stateLabel[step.state]}</span>
          </div>
          <pre class="code">{step.detail}</pre>
          {#if step.preview}<pre class="diff">{step.preview}</pre>{/if}
          {#if step.state === "waiting"}
            <div class="approve">
              <button class="btn small" onclick={() => chat.answer(step.id, false)}>Deny</button>
              <button class="btn small primary" onclick={() => chat.answer(step.id, true)}>Approve</button>
            </div>
          {/if}
          {#if step.result}
            <details>
              <summary>{step.state === "denied" || step.state === "failed" ? step.result.split("\n")[0] : "Output"}</summary>
              <pre class="out">{step.result}</pre>
            </details>
          {/if}
        </div>
      {/if}
    {/each}
    {#if chat.running && !chat.steps.some((s) => s.kind === "tool" && s.state === "waiting")}
      <p class="note-line"><span class="spinner"></span> Thinking…</p>
    {/if}
  </div>

  <footer>
    <label class="check" title="Commands that only look (ls, df, cat, ps, grep…) run without asking. Anything that changes something always asks.">
      <input type="checkbox" checked={chat.autoRead} onchange={(e) => chat.setAutoRead(e.currentTarget.checked)} />
      Run read-only commands without asking
    </label>
    <label class="check" title="Commands are typed into your own terminal, so you can watch them, and share its folder and variables. They are still shown to you for approval, and only POSIX shells such as bash and zsh are supported.">
      <input type="checkbox" checked={chat.useTerminal} disabled={!context.terminalSession} onchange={(e) => chat.setUseTerminal(e.currentTarget.checked)} />
      Run its commands in my terminal (visible)
    </label>
    <div class="composer">
      <textarea
        rows="2"
        placeholder="Ask about this server…  (Enter to send, Shift+Enter for a new line)"
        bind:value={draft}
        {onkeydown}
        disabled={!chat.providerId}
      ></textarea>
      {#if chat.running}
        <button class="btn danger" onclick={() => chat.stop()}>Stop</button>
      {:else}
        <button class="btn primary" disabled={!draft.trim() || !chat.providerId} onclick={submit}>Send</button>
      {/if}
    </div>
    {#if chat.usage.input + chat.usage.output > 0}
      <p class="usage">Used {(chat.usage.input + chat.usage.output).toLocaleString()} tokens in this conversation.</p>
    {/if}
  </footer>

  {#if showSettings}
    <AgentSettings onclose={() => (showSettings = false)} onchanged={changed} />
  {/if}
</aside>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
    background: var(--bg-1);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 8px 4px 12px;
  }
  .heading {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
  }
  .head-tools {
    display: flex;
  }
  .setup {
    padding: 0 12px 6px;
  }
  .setup select {
    width: 100%;
  }
  .privacy,
  .where {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: 11px;
  }
  .message.warn {
    margin: 6px 0 0;
    font-size: 12px;
  }
  .link {
    padding: 0;
    color: var(--accent);
    background: none;
    border: none;
    cursor: pointer;
    text-decoration: underline;
  }
  .log {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 12px;
  }
  .empty {
    color: var(--text-muted);
    font-size: 12px;
  }
  .examples {
    font-style: italic;
  }
  .bubble {
    margin: 6px 0;
    padding: 7px 10px;
    border-radius: 10px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 13px;
    line-height: 1.45;
  }
  .bubble.user {
    margin-left: 28px;
    background: var(--accent-soft);
  }
  .bubble.assistant {
    margin-right: 12px;
    background: var(--bg-2);
  }
  .note-line {
    margin: 6px 0;
    color: var(--text-muted);
    font-size: 12px;
    font-style: italic;
  }
  .spinner {
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 4px;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .tool {
    margin: 8px 0;
    padding: 8px;
    background: var(--bg-0, var(--bg-2));
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    font-size: 12px;
  }
  .tool.waiting {
    border-color: var(--accent);
  }
  .tool.bad {
    border-color: var(--danger);
  }
  .tool-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }
  .state {
    margin-left: auto;
    color: var(--text-muted);
  }
  .badge {
    padding: 1px 7px;
    font-size: 10px;
    border-radius: 999px;
    background: var(--bg-2);
    color: var(--text-muted);
  }
  .badge.changes {
    color: var(--warning, #d29922);
    background: var(--warning-soft, var(--bg-2));
  }
  .badge.blocked {
    color: var(--danger);
  }
  pre {
    margin: 4px 0;
    padding: 6px 8px;
    overflow: auto;
    max-height: 220px;
    background: var(--bg-2);
    border-radius: 4px;
    font: 12px var(--mono, monospace);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .approve {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 6px;
  }
  details summary {
    color: var(--text-muted);
    cursor: pointer;
  }
  footer {
    padding: 6px 12px 10px;
    border-top: 1px solid var(--border);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
    color: var(--text-muted);
    font-size: 12px;
  }
  .composer {
    display: flex;
    gap: 6px;
    align-items: flex-end;
  }
  .composer textarea {
    flex: 1;
    resize: vertical;
    min-height: 40px;
    max-height: 200px;
  }
  .usage {
    margin: 4px 0 0;
    color: var(--text-faint);
    font-size: 11px;
  }
</style>
