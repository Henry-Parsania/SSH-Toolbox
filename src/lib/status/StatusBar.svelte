<script lang="ts">
  import { statsLocal, statsRemote, type Stats } from "../backend/status";
  import { settings } from "../settings.svelte";
  import Spark from "../docker/Spark.svelte";
  import { bytes, percent, rate, uptime } from "./format";
  import { netSpeed, pushReading, type NetSample } from "./history";
  import Icon from "../ui/Icon.svelte";
  import { fitMenu } from "../ui/fitMenu";
  import { METRICS, type Part } from "./prefs";
  import { footerPrefs, resetFooter, setPart } from "./prefs.svelte";

  const PARTS: { id: Part; label: string }[] = [
    { id: "bar", label: "Bar" },
    { id: "numbers", label: "Numbers" },
    { id: "graph", label: "Graph" },
  ];
  const prefs = $derived(footerPrefs.value);
  // The footer's choice box, when open: where it was asked for.
  let menu = $state<{ x: number; y: number } | null>(null);

  interface Props {
    /** Whose load to show: a connected SSH session, or this computer. */
    target: { kind: "ssh"; id: string; name: string } | { kind: "local" };
  }

  let { target }: Props = $props();

  let stats = $state<Stats | null>(null);
  let note = $state("");

  // Recent readings for the small graphs, and the last network counters to work the speed out from.
  let cpuHistory = $state<number[]>([]);
  let ramHistory = $state<number[]>([]);
  let netHistory = $state<number[]>([]);
  let gpuHistory = $state<number[]>([]);
  let speed = $state<{ rx: number; tx: number } | null>(null);
  let lastNet: NetSample | null = null;

  const key = $derived(target.kind === "ssh" ? `ssh:${target.id}` : "local");

  // Reads now, then every few seconds. Changing what is shown starts again from nothing, so one
  // computer's numbers are never shown under another's name.
  $effect(() => {
    void key;
    const every = Math.max(2, settings.stats_interval_secs) * 1000;
    const wanted = $state.snapshot(target);
    stats = null;
    note = "";
    cpuHistory = [];
    ramHistory = [];
    netHistory = [];
    gpuHistory = [];
    speed = null;
    lastNet = null;
    let stopped = false;
    let busy = false;

    async function read() {
      if (busy || stopped) return;
      busy = true;
      try {
        const got = wanted.kind === "ssh" ? await statsRemote(wanted.id) : await statsLocal();
        if (stopped) return;
        stats = got;
        if (got) record(got);
        note = got ? "" : wanted.kind === "ssh" ? "No readings from this server" : "";
      } catch {
        if (!stopped) {
          stats = null;
          note = "Could not read the server";
        }
      } finally {
        busy = false;
      }
    }

    void read();
    const timer = setInterval(read, every);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  });

  /** Adds a reading to the graphs. */
  function record(got: Stats) {
    if (got.cpu_percent !== null) cpuHistory = pushReading(cpuHistory, got.cpu_percent);
    ramHistory = pushReading(ramHistory, percent(got.mem_used, got.mem_total));
    if (got.net_rx_bytes !== null && got.net_tx_bytes !== null) {
      const now = { at: performance.now(), rx: got.net_rx_bytes, tx: got.net_tx_bytes };
      const s = netSpeed(lastNet, now);
      lastNet = now;
      if (s) {
        speed = s;
        netHistory = pushReading(netHistory, s.rx + s.tx);
      }
    }
    if (got.gpu_percent !== null) gpuHistory = pushReading(gpuHistory, got.gpu_percent);
  }

  const memPct = $derived(stats ? percent(stats.mem_used, stats.mem_total) : 0);
  const diskPct = $derived(stats ? percent(stats.disk_used, stats.disk_total) : 0);
  const cpu = $derived(stats?.cpu_percent ?? null);

  function level(p: number): string {
    return p >= 90 ? "hot" : p >= 70 ? "warm" : "";
  }
</script>

<svelte:window onpointerdown={() => (menu = null)} onblur={() => (menu = null)} onkeydown={(e) => e.key === "Escape" && (menu = null)} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<footer
  class="status"
  aria-label="Load"
  oncontextmenu={(e) => {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY };
  }}
>
  <span class="where">{target.kind === "ssh" ? target.name : "This computer"}</span>
  {#if stats}
    {#if prefs.cpu.show}
      <span class="metric" title="Processor in use">
        <span class="label">CPU</span>
        {#if prefs.cpu.bar}<span class="bar"><span class="fill {level(cpu ?? 0)}" style:width="{cpu ?? 0}%"></span></span>{/if}
        {#if prefs.cpu.numbers}<span class="value">{cpu === null ? "…" : `${Math.round(cpu)}%`}</span>{/if}
        {#if prefs.cpu.graph}<Spark values={cpuHistory} max={100} width={44} height={14} label="Processor use over the last minutes" />{/if}
      </span>
    {/if}
    {#if prefs.ram.show}
      <span class="metric" title="Memory in use">
        <span class="label">RAM</span>
        {#if prefs.ram.bar}<span class="bar"><span class="fill {level(memPct)}" style:width="{memPct}%"></span></span>{/if}
        {#if prefs.ram.numbers}<span class="value">{bytes(stats.mem_used)} / {bytes(stats.mem_total)}</span>{/if}
        {#if prefs.ram.graph}<Spark values={ramHistory} max={100} width={44} height={14} label="Memory use over the last minutes" />{/if}
      </span>
    {/if}
    {#if prefs.net.show && stats.net_rx_bytes !== null}
      <span class="metric" title="Network traffic of all connections (received, sent)">
        <span class="label">Net</span>
        {#if prefs.net.numbers}<span class="value">{#if speed}<span class="rx">↓ {rate(speed.rx)}</span> <span class="tx">↑ {rate(speed.tx)}</span>{:else}…{/if}</span>{/if}
        {#if prefs.net.graph}<Spark values={netHistory} width={44} height={14} label="Network traffic over the last minutes" />{/if}
      </span>
    {/if}
    {#if prefs.gpu.show && stats.gpu_percent !== null}
      <span class="metric" title={`Graphics card in use (NVIDIA)${stats.gpu_mem_total ? `, memory ${bytes(stats.gpu_mem_used ?? 0)} of ${bytes(stats.gpu_mem_total)}` : ""}`}>
        <span class="label">GPU</span>
        {#if prefs.gpu.bar}<span class="bar"><span class="fill {level(stats.gpu_percent)}" style:width="{stats.gpu_percent}%"></span></span>{/if}
        {#if prefs.gpu.numbers}<span class="value">{Math.round(stats.gpu_percent)}%</span>{/if}
        {#if prefs.gpu.graph}<Spark values={gpuHistory} max={100} width={44} height={14} label="Graphics card use over the last minutes" />{/if}
      </span>
    {/if}
    {#if prefs.disk.show && stats.disk_total > 0}
      <span class="metric" title="Disk in use">
        <span class="label">Disk</span>
        {#if prefs.disk.bar}<span class="bar"><span class="fill {level(diskPct)}" style:width="{diskPct}%"></span></span>{/if}
        {#if prefs.disk.numbers}<span class="value">{bytes(stats.disk_used)} / {bytes(stats.disk_total)}</span>{/if}
      </span>
    {/if}
    {#if prefs.load.show && stats.load}
      <span class="metric" title="Load over 1, 5 and 15 minutes">
        <span class="label">Load</span>{#if prefs.load.numbers}<span class="value">{stats.load}</span>{/if}
      </span>
    {/if}
    {#if prefs.uptime.show && stats.uptime_secs !== null}
      <span class="metric" title="Time since the computer started">
        <span class="label">Up</span>{#if prefs.uptime.numbers}<span class="value">{uptime(stats.uptime_secs)}</span>{/if}
      </span>
    {/if}
  {:else}
    <span class="note">{note || "Reading…"}</span>
  {/if}
  <button class="adjust" title="Choose what the footer shows (or right-click it)" aria-label="Adjust the footer" onclick={(e) => {
    e.stopPropagation();
    const box = e.currentTarget.getBoundingClientRect();
    menu = { x: box.left, y: box.top - 4 };
  }}>
    <Icon name="settings" size={12} />
  </button>
</footer>

{#if menu}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="picker" role="dialog" aria-label="Footer" tabindex="-1" use:fitMenu={{ x: menu.x, y: menu.y }} onpointerdown={(e) => e.stopPropagation()} oncontextmenu={(e) => e.preventDefault()}>
    <div class="row head">
      <span class="name">What the footer shows</span>
      {#each PARTS as part (part.id)}<span class="cell">{part.label}</span>{/each}
    </div>
    {#each METRICS as m (m.id)}
      <div class="row">
        <label class="name"><input type="checkbox" checked={prefs[m.id].show} onchange={(e) => setPart(m.id, "show", e.currentTarget.checked)} /> {m.label}</label>
        {#each PARTS as part (part.id)}
          <span class="cell">
            {#if m.parts.includes(part.id)}
              <input type="checkbox" aria-label="{m.label}: {part.label}" disabled={!prefs[m.id].show} checked={prefs[m.id][part.id]} onchange={(e) => setPart(m.id, part.id, e.currentTarget.checked)} />
            {/if}
          </span>
        {/each}
      </div>
    {/each}
    <div class="foot"><button class="btn small" onclick={resetFooter}>Back to the defaults</button></div>
  </div>
{/if}

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 26px;
    padding: 0 12px;
    overflow: hidden;
    white-space: nowrap;
    color: var(--text-muted);
    background: var(--bg-1);
    border-top: 1px solid var(--border);
    font-size: 12px;
    user-select: none;
  }
  .where {
    font-weight: 600;
    color: var(--text);
  }
  .metric {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .label {
    color: var(--text-faint);
    font-weight: 500;
  }
  .rx {
    color: var(--accent);
  }
  .tx {
    color: var(--go);
  }
  .value {
    font-variant-numeric: tabular-nums;
  }
  .bar {
    width: 54px;
    height: 6px;
    overflow: hidden;
    background: var(--bg-active);
    border-radius: 999px;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
    transition: width 0.4s var(--ease);
  }
  .fill.warm {
    background: var(--warning);
  }
  .fill.hot {
    background: var(--danger);
  }
  .note {
    color: var(--text-faint);
  }
  .adjust {
    display: grid;
    place-items: center;
    margin-left: auto;
    padding: 2px;
    color: var(--text-faint);
    background: transparent;
    border: 0;
    border-radius: 4px;
    cursor: pointer;
  }
  .adjust:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .picker {
    position: fixed;
    z-index: 70;
    min-width: 330px;
    padding: 8px 10px 10px;
    color: var(--text);
    font-size: 12px;
    background: var(--bg-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 52px 64px 52px;
    align-items: center;
    padding: 3px 0;
  }
  .row.head {
    color: var(--text-faint);
    font-size: 11px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 3px;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .cell {
    text-align: center;
  }
  .foot {
    margin-top: 8px;
    text-align: right;
  }
</style>
