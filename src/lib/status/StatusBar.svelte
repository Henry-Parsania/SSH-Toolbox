<script lang="ts">
  import { statsLocal, statsRemote, type Stats } from "../backend/status";
  import { settings } from "../settings.svelte";
  import Spark from "../docker/Spark.svelte";
  import { bytes, percent, rate, uptime } from "./format";
  import { netSpeed, pushReading, type NetSample } from "./history";

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

<footer class="status" aria-label="Load">
  <span class="where">{target.kind === "ssh" ? target.name : "This computer"}</span>
  {#if stats}
    <span class="metric" title="Processor in use">
      <span class="label">CPU</span>
      <span class="bar"><span class="fill {level(cpu ?? 0)}" style:width="{cpu ?? 0}%"></span></span>
      <span class="value">{cpu === null ? "…" : `${Math.round(cpu)}%`}</span>
      <Spark values={cpuHistory} max={100} width={44} height={14} label="Processor use over the last minutes" />
    </span>
    <span class="metric" title="Memory in use">
      <span class="label">RAM</span>
      <span class="bar"><span class="fill {level(memPct)}" style:width="{memPct}%"></span></span>
      <span class="value">{bytes(stats.mem_used)} / {bytes(stats.mem_total)}</span>
      <Spark values={ramHistory} max={100} width={44} height={14} label="Memory use over the last minutes" />
    </span>
    {#if stats.net_rx_bytes !== null}
      <span class="metric" title="Network traffic of all connections (received, sent)">
        <span class="label">Net</span>
        <span class="value">{speed ? `↓ ${rate(speed.rx)}  ↑ ${rate(speed.tx)}` : "…"}</span>
        <Spark values={netHistory} width={44} height={14} label="Network traffic over the last minutes" />
      </span>
    {/if}
    {#if stats.gpu_percent !== null}
      <span class="metric" title="Graphics card in use (NVIDIA){stats.gpu_mem_total ? `, memory ${bytes(stats.gpu_mem_used ?? 0)} of ${bytes(stats.gpu_mem_total)}` : ''}">
        <span class="label">GPU</span>
        <span class="bar"><span class="fill {level(stats.gpu_percent)}" style:width="{stats.gpu_percent}%"></span></span>
        <span class="value">{Math.round(stats.gpu_percent)}%</span>
        <Spark values={gpuHistory} max={100} width={44} height={14} label="Graphics card use over the last minutes" />
      </span>
    {/if}
    {#if stats.disk_total > 0}
      <span class="metric" title="Disk in use">
        <span class="label">Disk</span>
        <span class="bar"><span class="fill {level(diskPct)}" style:width="{diskPct}%"></span></span>
        <span class="value">{bytes(stats.disk_used)} / {bytes(stats.disk_total)}</span>
      </span>
    {/if}
    {#if stats.load}
      <span class="metric" title="Load over 1, 5 and 15 minutes">
        <span class="label">Load</span><span class="value">{stats.load}</span>
      </span>
    {/if}
    {#if stats.uptime_secs !== null}
      <span class="metric" title="Time since the computer started">
        <span class="label">Up</span><span class="value">{uptime(stats.uptime_secs)}</span>
      </span>
    {/if}
  {:else}
    <span class="note">{note || "Reading…"}</span>
  {/if}
</footer>

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
</style>
