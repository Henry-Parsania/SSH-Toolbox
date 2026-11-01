<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { keySetupCancel, promptRespond, type Prompt, type PromptResponse, type SessionSpec } from "../backend";
  import {
    dockerAction,
    dockerClose,
    dockerConnect,
    dockerInspect,
    dockerList,
    type ContainerAction,
    type ContainerDetails,
    type ContainerInfo,
  } from "../backend/docker";
  import type { Library } from "../sessions/library.svelte";
  import PromptDialog from "../terminal/PromptDialog.svelte";
  import Icon from "../ui/Icon.svelte";

  interface Props {
    library: Library;
    /** Opens a container's log or shell in a new tab. */
    onopen: (spec: SessionSpec, title: string) => void;
    onclose: () => void;
  }

  let { library, onopen, onclose }: Props = $props();

  const REFRESH_MS = 4000;
  const LOCAL = "local";

  const servers = $derived(library.file.session.filter((s) => s.type === "ssh"));
  let source = $state(localStorage.getItem("docker-source") ?? LOCAL);
  // A server that was deleted since last time falls back to this computer.
  const chosen = $derived(source === LOCAL || servers.some((s) => s.id === source) ? source : LOCAL);

  let dockerId = $state<string | null>(null);
  let connecting = $state(false);
  let error = $state("");
  let containers = $state<ContainerInfo[]>([]);
  let busy = $state<Record<string, boolean>>({});
  let question = $state<{ id: string; prompt: Prompt } | null>(null);
  let operation = "";

  let details = $state<{ name: string; data: ContainerDetails | null; error: string; tab: string } | null>(null);
  let removing = $state<ContainerInfo | null>(null);
  let filter = $state("");

  async function connect() {
    await disconnect();
    connecting = true;
    error = "";
    operation = crypto.randomUUID();
    const target =
      chosen === LOCAL ? ({ type: "local" } as const) : ({ type: "ssh", saved_session_id: chosen } as const);
    try {
      const id = await dockerConnect(target, operation, (status) => {
        if (status.type === "prompt") question = { id: status.prompt_id, prompt: status.prompt };
      });
      dockerId = id;
      await refresh();
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't connect to Docker.";
    } finally {
      question = null;
      connecting = false;
    }
  }

  async function disconnect() {
    const id = dockerId;
    dockerId = null;
    containers = [];
    details = null;
    if (id) await dockerClose(id).catch(() => {});
  }

  function cancelConnecting() {
    if (operation) void keySetupCancel(operation).catch(() => {});
  }

  async function refresh() {
    const id = dockerId;
    if (!id) return;
    try {
      const list = await dockerList(id);
      if (dockerId === id) {
        containers = list;
        error = "";
      }
    } catch (err) {
      if (dockerId === id) error = (err as { message?: string }).message ?? "Couldn't read the containers.";
    }
  }

  // The list keeps itself up to date while the panel is connected and the window is showing.
  $effect(() => {
    if (!dockerId) return;
    const timer = setInterval(() => {
      if (!document.hidden && !removing) untrack(() => void refresh());
    }, REFRESH_MS);
    return () => clearInterval(timer);
  });

  onDestroy(() => void disconnect());

  function choose(id: string) {
    source = id;
    localStorage.setItem("docker-source", id);
    void disconnect();
    error = "";
  }

  async function act(container: ContainerInfo, action: ContainerAction) {
    const id = dockerId;
    if (!id) return;
    busy[container.id] = true;
    try {
      await dockerAction(id, container.id, action);
      error = "";
    } catch (err) {
      error = (err as { message?: string }).message ?? "That didn't work.";
    } finally {
      delete busy[container.id];
      await refresh();
    }
  }

  async function confirmRemove() {
    const target = removing;
    removing = null;
    if (target) await act(target, "remove");
  }

  async function showDetails(container: ContainerInfo) {
    const id = dockerId;
    if (!id) return;
    details = { name: container.name, data: null, error: "", tab: "summary" };
    try {
      const data = await dockerInspect(id, container.id);
      if (details) details = { ...details, data };
    } catch (err) {
      if (details) details = { ...details, error: (err as { message?: string }).message ?? "Couldn't read the details." };
    }
  }

  function open(kind: "docker_logs" | "docker_shell", container: ContainerInfo) {
    if (!dockerId) return;
    const word = kind === "docker_logs" ? "logs" : "shell";
    onopen({ type: kind, docker_id: dockerId, container_id: container.id }, `${container.name} ${word}`);
  }

  function answer(response: PromptResponse) {
    if (!question) return;
    void promptRespond(question.id, response).catch(() => {});
    question = null;
  }

  const visible = $derived.by(() => {
    const word = filter.trim().toLowerCase();
    return word
      ? containers.filter((c) => `${c.name} ${c.image} ${c.project ?? ""}`.toLowerCase().includes(word))
      : containers;
  });

  // Compose projects first, by name, and then everything else.
  const groups = $derived.by(() => {
    const byProject = new Map<string, ContainerInfo[]>();
    const others: ContainerInfo[] = [];
    for (const c of visible) {
      if (c.project) byProject.set(c.project, [...(byProject.get(c.project) ?? []), c]);
      else others.push(c);
    }
    const out = [...byProject.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([name, items]) => ({ name, items }));
    if (others.length) out.push({ name: out.length ? "Other containers" : "", items: others });
    return out;
  });

  const running = $derived(containers.filter((c) => c.state === "running").length);

  function dot(state: string): string {
    if (state === "running") return "ok";
    if (state === "paused" || state === "restarting") return "warn";
    return "off";
  }

  const detailTabs = [
    ["summary", "Summary"],
    ["env", "Environment"],
    ["ports", "Ports"],
    ["mounts", "Mounts"],
    ["networks", "Networks"],
    ["raw", "Raw"],
  ] as const;

  function lines(data: ContainerDetails, tab: string): string {
    switch (tab) {
      case "env":
        return data.env.join("\n") || "No environment variables.";
      case "ports":
        return data.ports.join("\n") || "No ports.";
      case "mounts":
        return data.mounts.join("\n") || "No mounts.";
      case "networks":
        return data.networks.join("\n") || "No networks.";
      case "raw":
        return data.raw;
      default:
        return `Name: ${data.name}\nImage: ${data.image}\nState: ${data.state}\nTerminal: ${data.tty ? "yes" : "no"}`;
    }
  }
</script>

<aside class="panel" aria-label="Docker">
  <header>
    <span class="heading"><Icon name="box" size={16} /> Docker</span>
    <div class="head-tools">
      {#if dockerId}
        <button class="icon-btn" aria-label="Refresh" title="Refresh" onclick={refresh}>
          <Icon name="refresh" size={15} />
        </button>
      {/if}
      <button class="icon-btn" aria-label="Close Docker" title="Close" onclick={onclose}>
        <Icon name="close" size={15} />
      </button>
    </div>
  </header>

  <div class="source">
    <select
      aria-label="Where Docker runs"
      value={chosen}
      onchange={(e) => choose((e.currentTarget as HTMLSelectElement).value)}
    >
      <option value={LOCAL}>This computer</option>
      {#each servers as s (s.id)}
        <option value={s.id}>{s.name}</option>
      {/each}
    </select>
    {#if dockerId}
      <button class="btn small" onclick={disconnect}>Disconnect</button>
    {:else if connecting}
      <button class="btn small" onclick={cancelConnecting}>Cancel</button>
    {:else}
      <button class="btn small primary" onclick={connect}>Connect</button>
    {/if}
  </div>

  {#if error}<p class="message error">{error}</p>{/if}

  {#if connecting}
    <p class="hint">Connecting…</p>
  {:else if dockerId}
    <div class="bar">
      <input type="search" placeholder="Filter containers" bind:value={filter} aria-label="Filter containers" />
      <span class="count">{running} of {containers.length} running</span>
    </div>
    <div class="list">
      {#each groups as group (group.name)}
        {#if group.name}<h3>{group.name}</h3>{/if}
        {#each group.items as c (c.id)}
          <div class="row" class:dim={c.state !== "running"}>
            <span class="dot {dot(c.state)}" title={c.state}></span>
            <div class="what">
              <span class="name">{c.name}</span>
              <span class="sub">{c.image} · {c.status}</span>
              {#if c.ports.length}<span class="sub">{c.ports.join(", ")}</span>{/if}
            </div>
            <div class="actions">
              {#if c.state === "running"}
                <button class="icon-btn small" title="Stop" disabled={busy[c.id]} onclick={() => act(c, "stop")}>
                  <Icon name="stop" size={13} />
                </button>
                <button class="icon-btn small" title="Restart" disabled={busy[c.id]} onclick={() => act(c, "restart")}>
                  <Icon name="refresh" size={13} />
                </button>
                <button class="icon-btn small" title="Open a shell" onclick={() => open("docker_shell", c)}>
                  <Icon name="terminal" size={13} />
                </button>
              {:else}
                <button class="icon-btn small" title="Start" disabled={busy[c.id]} onclick={() => act(c, "start")}>
                  <Icon name="play" size={13} />
                </button>
              {/if}
              <button class="icon-btn small" title="Show the log" onclick={() => open("docker_logs", c)}>
                <Icon name="log" size={13} />
              </button>
              <button class="icon-btn small" title="Details" onclick={() => showDetails(c)}>
                <Icon name="info" size={13} />
              </button>
              <button class="icon-btn small" title="Remove" disabled={busy[c.id]} onclick={() => (removing = c)}>
                <Icon name="trash" size={13} />
              </button>
            </div>
          </div>
        {/each}
      {:else}
        <p class="hint">{containers.length ? "No container matches." : "There are no containers."}</p>
      {/each}
    </div>
  {:else if !error}
    <p class="hint">Choose where Docker runs and press Connect. For a server, nothing is installed there; its user needs access to Docker.</p>
  {/if}

  {#if question}
    <PromptDialog prompt={question.prompt} onanswer={answer} />
  {/if}

  {#if removing}
    <div class="modal-backdrop">
      <div class="modal" role="alertdialog" aria-label="Remove container" tabindex="-1">
        <h2>Remove “{removing.name}”?</h2>
        <p>
          The container is deleted, along with anything stored only inside it. Its image and named volumes stay.
          {#if removing.state === "running"}It is running, so Docker will refuse. Stop it first.{/if}
        </p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (removing = null)}>Cancel</button>
          <button class="btn danger" onclick={confirmRemove}>Remove</button>
        </div>
      </div>
    </div>
  {/if}

  {#if details}
    <div class="modal-backdrop">
      <div class="modal wide" role="dialog" aria-label="Container details" tabindex="-1">
        <h2>{details.name}</h2>
        {#if details.error}
          <p class="message error">{details.error}</p>
        {:else if !details.data}
          <p class="hint">Reading…</p>
        {:else}
          <div class="tabs" role="tablist">
            {#each detailTabs as [id, label] (id)}
              <button
                role="tab"
                class="tab"
                class:on={details.tab === id}
                aria-selected={details.tab === id}
                onclick={() => details && (details = { ...details, tab: id })}>{label}</button
              >
            {/each}
          </div>
          <pre>{lines(details.data, details.tab)}</pre>
        {/if}
        <div class="modal-actions">
          <button class="btn" onclick={() => (details = null)}>Close</button>
        </div>
      </div>
    </div>
  {/if}
</aside>

<style>
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 100%;
    flex: none;
    min-height: 0;
    background: var(--bg-1);
    border-left: 1px solid var(--border);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 8px 4px 14px;
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
    gap: 2px;
  }
  .source {
    display: flex;
    gap: 6px;
    margin: 6px 12px;
  }
  .source select {
    flex: 1;
    min-width: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 2px 12px 6px;
  }
  .bar input {
    flex: 1;
    min-width: 0;
  }
  .count {
    flex: none;
    color: var(--text-muted);
    font-size: 11px;
  }
  .message,
  .hint {
    margin: 6px 14px;
    color: var(--text-muted);
    font-size: 12px;
  }
  .message.error {
    color: var(--danger);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 6px 10px;
  }
  h3 {
    margin: 10px 8px 4px;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-small, 6px);
  }
  .row:hover {
    background: var(--bg-2);
  }
  .row.dim .name {
    color: var(--text-muted);
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--text-muted);
  }
  .dot.ok {
    background: var(--success, #3fb950);
  }
  .dot.warn {
    background: var(--warning, #d29922);
  }
  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
    font-size: 11px;
  }
  .actions {
    display: flex;
    flex: none;
    opacity: 0.55;
  }
  .row:hover .actions,
  .actions:focus-within {
    opacity: 1;
  }
  .modal.wide {
    width: min(640px, 92vw);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 8px;
  }
  .tab {
    padding: 3px 10px;
    color: var(--text-muted);
    background: transparent;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    font-size: 12px;
    cursor: pointer;
  }
  .tab.on {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }
  pre {
    max-height: 50vh;
    margin: 0 0 10px;
    padding: 10px;
    overflow: auto;
    background: var(--bg-0, var(--bg-2));
    border-radius: 6px;
    font-family: var(--mono, monospace);
    font-size: 12px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
