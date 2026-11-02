<script lang="ts">
  // The network scanner (find computers and what they offer) and the port scanner (see what one
  // computer offers), in the style of MobaXterm's tools.
  import { onDestroy, onMount } from "svelte";
  import { scanLocalNetworks, type HostResult } from "../backend/scan";
  import Icon from "../ui/Icon.svelte";
  import { ScanJob } from "./job.svelte";

  interface Props {
    initialTab?: "network" | "ports";
    onclose: () => void;
    /** Starts a new SSH session for an address, in the sessions list. */
    onnewsession: (host: string, port: number) => void;
  }

  let { initialTab = "network", onclose, onnewsession }: Props = $props();

  type Tab = "network" | "ports";
  // The tab it opens on; after that the user chooses.
  // svelte-ignore state_referenced_locally
  let tab = $state<Tab>(initialTab);

  const speeds = [
    { label: "Fast (0.4 s)", ms: 400 },
    { label: "Normal (0.8 s)", ms: 800 },
    { label: "Careful (2.5 s)", ms: 2500 },
  ];
  const DISCOVERY = "22,80,443,445,139,135,3389,5900,21,23,8080,53,631,62078";

  // ---- Network scanner
  const net = new ScanJob();
  let netTarget = $state("");
  let netPorts = $state<"discovery" | "common" | "custom">("discovery");
  let netCustom = $state("22,80,443");
  let netSpeed = $state(800);
  let netBanners = $state(false);
  let suggestions = $state<string[]>([]);

  // ---- Port scanner
  const ports = new ScanJob();
  let portHost = $state("");
  let portPreset = $state<"common" | "1-1024" | "all" | "custom">("common");
  let portCustom = $state("1-1024");
  let portSpeed = $state(800);
  let portBanners = $state(true);

  onMount(async () => {
    suggestions = await scanLocalNetworks().catch(() => []);
    if (!netTarget && suggestions.length) netTarget = suggestions[0];
  });
  onDestroy(() => {
    net.stop();
    ports.stop();
  });

  function startNetwork() {
    const spec = netPorts === "discovery" ? DISCOVERY : netPorts === "common" ? "common" : netCustom;
    void net.start({ targets: netTarget, ports: spec, timeout_ms: netSpeed, banners: netBanners, only_alive: true });
  }

  function startPorts() {
    const spec = portPreset === "custom" ? portCustom : portPreset;
    void ports.start({ targets: portHost, ports: spec, timeout_ms: portSpeed, banners: portBanners, only_alive: false });
  }

  function scanPortsOf(host: string) {
    portHost = host;
    tab = "ports";
    startPorts();
  }

  function newSession(host: string) {
    onclose();
    onnewsession(host, 22);
  }

  const chips = (h: HostResult) => h.open.map((p) => ({ text: `${p.service ?? "port"} ${p.port}`, title: p.banner ?? "" }));

  function copy(text: string) {
    void navigator.clipboard.writeText(text).catch(() => {});
  }

  function asText(job: ScanJob, withPorts: boolean): string {
    return job.sorted
      .map((h) => (withPorts ? `${h.ip}\t${h.open.map((p) => `${p.port}${p.service ? `/${p.service}` : ""}`).join(", ")}` : h.ip))
      .join("\n");
  }

  const percent = (job: ScanJob) => (job.progress.total > 0 ? Math.round((job.progress.done / job.progress.total) * 100) : 0);
  const current = $derived(tab === "network" ? net : ports);
</script>

<div class="modal-backdrop">
  <div class="modal wide scanner" role="dialog" aria-label="Network tools" tabindex="-1" onkeydown={(e) => e.key === "Escape" && onclose()}>
    <h2>Network tools</h2>
    <div class="tabs" role="tablist">
      <button role="tab" class="tab" class:on={tab === "network"} aria-selected={tab === "network"} onclick={() => (tab = "network")}>Network scanner</button>
      <button role="tab" class="tab" class:on={tab === "ports"} aria-selected={tab === "ports"} onclick={() => (tab = "ports")}>Port scanner</button>
    </div>
    <p class="warn">Only scan networks and computers that are yours or that you have permission to scan.</p>

    {#if tab === "network"}
      <div class="form">
        <label class="field grow">
          Network, range or addresses
          <input type="text" bind:value={netTarget} spellcheck="false" placeholder="192.168.1.0/24" disabled={net.running} onkeydown={(e) => e.key === "Enter" && !net.running && startNetwork()} />
        </label>
        <label class="field">
          Look for
          <select bind:value={netPorts} disabled={net.running}>
            <option value="discovery">Quick: find computers</option>
            <option value="common">Common services</option>
            <option value="custom">My ports…</option>
          </select>
        </label>
        <label class="field">
          Wait
          <select bind:value={netSpeed} disabled={net.running}>
            {#each speeds as s (s.ms)}<option value={s.ms}>{s.label}</option>{/each}
          </select>
        </label>
      </div>
      {#if netPorts === "custom"}
        <label class="field"><input type="text" bind:value={netCustom} spellcheck="false" placeholder="22,80,8000-8100" disabled={net.running} /></label>
      {/if}
      <div class="line">
        {#if suggestions.length}
          <span class="muted">This computer is on:</span>
          {#each suggestions as s (s)}<button class="chip" disabled={net.running} onclick={() => (netTarget = s)}>{s}</button>{/each}
        {/if}
        <span class="spacer"></span>
        <label class="check"><input type="checkbox" bind:checked={netBanners} disabled={net.running} /> Read greetings (slower)</label>
        {#if net.running}
          <button class="btn danger" onclick={() => net.stop()}>Stop</button>
        {:else}
          <button class="btn primary" disabled={!netTarget.trim()} onclick={startNetwork}><Icon name="search" size={13} /> Scan</button>
        {/if}
      </div>
    {:else}
      <div class="form">
        <label class="field grow">
          Computer (address or name)
          <input type="text" bind:value={portHost} spellcheck="false" placeholder="192.168.1.10 or server.local" disabled={ports.running} onkeydown={(e) => e.key === "Enter" && !ports.running && startPorts()} />
        </label>
        <label class="field">
          Ports
          <select bind:value={portPreset} disabled={ports.running}>
            <option value="common">Common services</option>
            <option value="1-1024">Well-known (1–1024)</option>
            <option value="all">All ports (1–65535, slow)</option>
            <option value="custom">My ports…</option>
          </select>
        </label>
        <label class="field">
          Wait
          <select bind:value={portSpeed} disabled={ports.running}>
            {#each speeds as s (s.ms)}<option value={s.ms}>{s.label}</option>{/each}
          </select>
        </label>
      </div>
      {#if portPreset === "custom"}
        <label class="field"><input type="text" bind:value={portCustom} spellcheck="false" placeholder="22,80,8000-8100" disabled={ports.running} /></label>
      {/if}
      <div class="line">
        <span class="spacer"></span>
        <label class="check"><input type="checkbox" bind:checked={portBanners} disabled={ports.running} /> Read greetings</label>
        {#if ports.running}
          <button class="btn danger" onclick={() => ports.stop()}>Stop</button>
        {:else}
          <button class="btn primary" disabled={!portHost.trim()} onclick={startPorts}><Icon name="search" size={13} /> Scan</button>
        {/if}
      </div>
    {/if}

    {#if current.running || current.progress.total > 0}
      <div class="bar" role="progressbar" aria-valuenow={percent(current)} aria-valuemin="0" aria-valuemax="100"><div class="fill" style:width="{percent(current)}%"></div></div>
    {/if}
    {#if current.error}<p class="message error">{current.error}</p>{/if}
    {#if current.summary}
      <p class="summary">
        {#if tab === "network"}
          {current.summary.up} of {current.summary.hosts} addresses answered · {current.summary.open_ports} open ports{current.summary.cancelled ? " · stopped" : ""}
        {:else}
          {current.summary.open_ports} open ports found{current.summary.cancelled ? " · stopped" : ""}
        {/if}
      </p>
    {:else if current.running}
      <p class="summary">Scanning… {current.hosts.length} found so far</p>
    {/if}

    <div class="results">
      {#if tab === "network"}
        {#if net.sorted.length}
          <table>
            <thead><tr><th>Address</th><th>Services</th><th></th></tr></thead>
            <tbody>
              {#each net.sorted as h (h.ip)}
                <tr>
                  <td class="mono">{h.ip}</td>
                  <td>
                    {#each chips(h) as c (c.text)}<span class="svc" title={c.title}>{c.text}</span>{/each}
                    {#if h.open.length === 0}<span class="muted">answers, no open service found</span>{/if}
                  </td>
                  <td class="acts">
                    {#if h.open.some((p) => p.port === 22)}
                      <button class="btn small" title="Add an SSH session for this address" onclick={() => newSession(h.ip)}>SSH…</button>
                    {/if}
                    <button class="btn small" title="Scan this computer's ports" onclick={() => scanPortsOf(h.ip)}>Ports</button>
                    <button class="icon-btn small" title="Copy the address" aria-label="Copy the address" onclick={() => copy(h.ip)}><Icon name="copy" size={13} /></button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else if !net.running && !net.summary}
          <p class="muted pad">Choose a network and press Scan. Computers that answer are listed with the services they offer.</p>
        {/if}
      {:else}
        {#each ports.sorted as h (h.ip)}
          <p class="host mono">{h.ip}</p>
          {#if h.open.length}
            <table>
              <thead><tr><th>Port</th><th>Service</th><th>Greeting</th><th></th></tr></thead>
              <tbody>
                {#each h.open as p (p.port)}
                  <tr>
                    <td class="mono">{p.port}</td>
                    <td>{p.service ?? ""}</td>
                    <td class="mono banner" title={p.banner ?? ""}>{p.banner ?? ""}</td>
                    <td class="acts">
                      {#if p.port === 22}<button class="btn small" onclick={() => newSession(h.ip)}>SSH…</button>{/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else if !ports.running}
            <p class="muted pad">No open ports were found{h.alive ? " (the computer answered, so it is there)" : " (it may be off, or a firewall may be hiding its ports)"}.</p>
          {/if}
        {:else}
          {#if !ports.running && !ports.summary}<p class="muted pad">Enter a computer and press Scan to see which ports are open.</p>{/if}
        {/each}
      {/if}
    </div>

    <div class="modal-actions">
      <button class="btn" disabled={current.hosts.length === 0} onclick={() => copy(asText(current, tab === "ports" || netPorts !== "discovery"))}>Copy results</button>
      <button class="btn" onclick={onclose}>Close</button>
    </div>
  </div>
</div>

<style>
  .scanner {
    width: min(860px, 94vw);
  }
  .tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 6px;
  }
  .tab {
    padding: 4px 14px;
    color: var(--text-muted);
    background: transparent;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    cursor: pointer;
  }
  .tab.on {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }
  .warn {
    margin: 0 0 8px;
    color: var(--text-muted);
    font-size: 12px;
  }
  .form {
    display: flex;
    gap: 10px;
    align-items: flex-end;
  }
  .grow {
    flex: 1;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0;
    flex-wrap: wrap;
  }
  .spacer {
    flex: 1;
  }
  .chip {
    padding: 1px 9px;
    font: 12px var(--mono, monospace);
    color: var(--text);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    cursor: pointer;
  }
  .muted {
    color: var(--text-muted);
    font-size: 12px;
  }
  .pad {
    padding: 14px 4px;
  }
  .bar {
    height: 6px;
    margin: 8px 0 4px;
    overflow: hidden;
    background: var(--bg-2);
    border-radius: 3px;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .summary {
    margin: 4px 0;
    color: var(--text-muted);
    font-size: 12px;
  }
  .results {
    max-height: 46vh;
    min-height: 120px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    position: sticky;
    top: 0;
    padding: 5px 10px;
    color: var(--text-muted);
    font-size: 11px;
    text-align: left;
    background: var(--bg-1);
  }
  td {
    padding: 4px 10px;
    border-top: 1px solid var(--border);
    vertical-align: middle;
  }
  .mono {
    font-family: var(--mono, monospace);
  }
  .banner {
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }
  .svc {
    display: inline-block;
    margin: 1px 4px 1px 0;
    padding: 0 8px;
    font-size: 12px;
    background: var(--accent-soft);
    border-radius: 999px;
  }
  .acts {
    white-space: nowrap;
    text-align: right;
  }
  .host {
    margin: 8px 10px 2px;
    font-weight: 650;
  }
</style>
