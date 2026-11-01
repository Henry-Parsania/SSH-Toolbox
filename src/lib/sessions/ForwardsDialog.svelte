<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    forwardList,
    forwardStart,
    forwardStop,
    keySetupCancel,
    promptRespond,
    type ForwardInfo,
    type ForwardSpec,
    type Prompt,
    type PromptResponse,
  } from "../backend";
  import PromptDialog from "../terminal/PromptDialog.svelte";
  import type { SshSession } from "./tree";

  interface Props {
    session: SshSession;
    onclose: () => void;
  }

  let { session, onclose }: Props = $props();

  type Kind = "local" | "remote" | "socks";
  let kind = $state<Kind>("local");
  let bindHost = $state("127.0.0.1");
  let bindPort = $state<string | number>("");
  let destHost = $state("localhost");
  let destPort = $state<string | number>("");

  let forwards = $state<ForwardInfo[]>([]);
  let error = $state("");
  let busy = $state(false);
  let question = $state<{ id: string; prompt: Prompt } | null>(null);
  let warnExternal = $state(false);
  let operationId = "";
  let timer: ReturnType<typeof setInterval> | undefined;

  function isLoopback(host: string): boolean {
    const h = host.trim().toLowerCase().replace(/^\[|\]$/g, "");
    return h === "localhost" || h === "::1" || /^127\.\d+\.\d+\.\d+$/.test(h);
  }

  async function refresh() {
    try {
      forwards = (await forwardList()).filter((f) => f.session_id === session.id);
    } catch {
      /* the list just stays as it was */
    }
  }

  onMount(() => {
    void refresh();
    // Forwards end by themselves when the connection drops, so keep the list current.
    timer = setInterval(refresh, 2000);
  });
  onDestroy(() => clearInterval(timer));

  function buildSpec(): ForwardSpec | string {
    const port = Number(bindPort === "" ? 0 : bindPort);
    if (!Number.isInteger(port) || port < 0 || port > 65535) return "The port must be between 0 and 65535.";
    if (!bindHost.trim()) return "Enter the address to listen on.";
    const base = { bind_host: bindHost.trim(), bind_port: port };
    if (kind === "socks") return { type: "socks", ...base };
    const dest = Number(destPort);
    if (!destHost.trim()) return "Enter where the connection should go.";
    if (!Number.isInteger(dest) || dest < 1 || dest > 65535) return "The destination port must be between 1 and 65535.";
    return { type: kind, dest_host: destHost.trim(), dest_port: dest, ...base };
  }

  async function start(confirmExternal = false) {
    const spec = buildSpec();
    if (typeof spec === "string") {
      error = spec;
      return;
    }
    // Listening beyond this computer lets others on the network use the connection: ask first.
    if (!isLoopback(bindHost) && !confirmExternal) {
      warnExternal = true;
      return;
    }
    warnExternal = false;
    busy = true;
    error = "";
    operationId = crypto.randomUUID();
    try {
      await forwardStart(operationId, session.id, spec, confirmExternal, (status) => {
        if (status.type === "prompt") question = { id: status.prompt_id, prompt: status.prompt };
      });
      await refresh();
    } catch (err) {
      error = (err as { message?: string }).message ?? "The forward couldn't be started.";
    } finally {
      busy = false;
      question = null;
    }
  }

  async function stop(id: string) {
    await forwardStop(id).catch(() => {});
    await refresh();
  }

  function answer(response: PromptResponse) {
    if (!question) return;
    void promptRespond(question.id, response).catch(() => {});
    question = null;
  }

  async function close() {
    if (busy) await keySetupCancel(operationId).catch(() => {});
    onclose();
  }

  function describe(f: ForwardInfo): string {
    const s = f.spec;
    if (s.type === "socks") return `SOCKS proxy on ${s.bind_host}:${f.port}`;
    if (s.type === "local") return `${s.bind_host}:${f.port} → ${s.dest_host}:${s.dest_port} (through the server)`;
    return `server ${s.bind_host}:${f.port} → ${s.dest_host}:${s.dest_port} (on this computer)`;
  }
</script>

<div class="modal-backdrop">
  <div
    class="modal wide"
    role="dialog"
    aria-label="Port forwarding"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape" && !question && !warnExternal) void close();
      e.stopPropagation();
    }}
  >
    <h2>Port forwarding for {session.name}</h2>

    {#if forwards.length}
      <ul class="running">
        {#each forwards as f (f.id)}
          <li>
            <span>{describe(f)}</span>
            <button class="btn small" onclick={() => stop(f.id)}>Stop</button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="lead">Nothing is being forwarded. Forwards last until you stop them or the connection ends.</p>
    {/if}

    <h3>Add one</h3>
    <label class="field">
      Kind
      <select bind:value={kind}>
        <option value="local">Local: a port here reaches something through the server</option>
        <option value="remote">Remote: a port on the server reaches something here</option>
        <option value="socks">SOCKS proxy: browse through the server</option>
      </select>
    </label>
    <div class="row">
      <label class="field grow">
        {kind === "remote" ? "Listen on (server)" : "Listen on (this computer)"}
        <input type="text" bind:value={bindHost} />
      </label>
      <label class="field port">
        Port
        <input type="number" min="0" max="65535" bind:value={bindPort} placeholder="any" />
      </label>
    </div>
    {#if kind !== "socks"}
      <div class="row">
        <label class="field grow">
          {kind === "remote" ? "Send to (this computer)" : "Send to (as the server sees it)"}
          <input type="text" bind:value={destHost} />
        </label>
        <label class="field port">
          Port
          <input type="number" min="1" max="65535" bind:value={destPort} />
        </label>
      </div>
    {/if}

    {#if warnExternal}
      <div class="warn">
        <p>
          <b>{bindHost}</b> is open to your network: other computers could use this connection.
          Only continue if you want that.
        </p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (warnExternal = false)}>Cancel</button>
          <button class="btn danger" onclick={() => start(true)}>Listen on the network</button>
        </div>
      </div>
    {/if}

    {#if error}<p class="message error">{error}</p>{/if}
    <div class="modal-actions">
      <button class="btn" onclick={close}>Close</button>
      <button class="btn primary" disabled={busy} onclick={() => start()}>
        {busy ? "Connecting…" : "Start"}
      </button>
    </div>

    {#if question}
      <PromptDialog prompt={question.prompt} onanswer={answer} />
    {/if}
  </div>
</div>

<style>
  .running {
    list-style: none;
    margin: 0;
    padding: 0;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .running li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-bottom: 1px solid var(--border);
    font: 12px var(--mono);
  }
  .running li:last-child {
    border-bottom: none;
  }
  .port {
    flex: none;
    width: 7.5em;
  }
  .warn {
    margin: 4px 0 12px;
    padding: 10px 12px;
    background: var(--warning-soft);
    border: 1px solid #f0b45a55;
    border-radius: var(--radius);
  }
  .warn p {
    margin: 0 0 8px;
  }
  .warn .modal-actions {
    margin-top: 6px;
  }
</style>
