<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { keySetupCancel, promptRespond, type Prompt, type PromptResponse, type SessionSpec } from "../backend";
  import {
    dockerAction,
    dockerClose,
    dockerConnect,
    dockerInspect,
    dockerList,
    dockerPrune,
    dockerPull,
    dockerRun,
    type PullProgress,
    type RunSpec,
    dockerRemoveResource,
    dockerRename,
    dockerResourceDetails,
    dockerResources,
    type PruneKind,
    type ResourceDetails,
    type DockerResource,
    type ResourceKind,
    dockerStats,
    type ContainerStats,
    type ContainerAction,
    type ContainerDetails,
    type ContainerInfo,
  } from "../backend/docker";
  import type { Library } from "../sessions/library.svelte";
  import PromptDialog from "../terminal/PromptDialog.svelte";
  import Icon from "../ui/Icon.svelte";
  import Spark from "./Spark.svelte";

  interface Props {
    library: Library;
    /** The SSH server in the active tab, if any. The panel follows it. */
    active: { id: string; name: string } | null;
    /** Opens a container's log or shell in a new tab. */
    onopen: (spec: SessionSpec, title: string) => void;
    onclose: () => void;
  }

  let { library, active, onopen, onclose }: Props = $props();

  const REFRESH_MS = 4000;
  const LOCAL = "local";

  // The panel follows the SSH server in the active tab. This computer's own Docker is used only
  // when asked for, until the next SSH tab is chosen.
  let useLocal = $state(false);
  const wanted = $derived(useLocal ? LOCAL : (active?.id ?? null));

  let dockerId = $state<string | null>(null);
  // What the panel is showing, for its heading.
  let shownName = $state("");
  let connecting = $state(false);
  let error = $state("");
  let containers = $state<ContainerInfo[]>([]);
  let busy = $state<Record<string, boolean>>({});
  let question = $state<{ id: string; prompt: Prompt } | null>(null);
  let operation = "";

  let details = $state<{ id: string; name: string; data: ContainerDetails | null; error: string; tab: string } | null>(null);
  let removing = $state<ContainerInfo | null>(null);
  let filter = $state("");
  let view = $state<"containers" | ResourceKind>("containers");
  let resources = $state<DockerResource[]>([]);
  let removingResource = $state<DockerResource | null>(null);

  async function loadResources() {
    const id = dockerId;
    if (!id || view === "containers") return;
    try {
      const list = await dockerResources(id, view);
      if (dockerId === id) resources = list;
    } catch (err) {
      if (dockerId === id) error = (err as { message?: string }).message ?? "Couldn't read the list.";
    }
  }

  function show(next: "containers" | ResourceKind) {
    view = next;
    resources = [];
    filter = "";
    void loadResources();
  }

  async function confirmRemoveResource() {
    const target = removingResource;
    removingResource = null;
    const id = dockerId;
    if (!target || !id || view === "containers") return;
    try {
      await dockerRemoveResource(id, view, target.id);
      error = "";
    } catch (err) {
      error = (err as { message?: string }).message ?? "That did not work.";
    }
    await loadResources();
  }

  const shownResources = $derived.by(() => {
    const word = filter.trim().toLowerCase();
    return word ? resources.filter((r) => `${r.name} ${r.detail}`.toLowerCase().includes(word)) : resources;
  });
  let stats = $state<Record<string, ContainerStats>>({});

  // ---- Right-click menu, and the dialogs it opens.
  type Menu = {
    x: number;
    y: number;
    kind: "container" | "resource" | "project";
    c?: ContainerInfo;
    r?: DockerResource;
    project?: string;
    items?: ContainerInfo[];
  };
  let menu = $state<Menu | null>(null);
  let note = $state("");
  let renaming = $state<{ c: ContainerInfo; value: string; error: string } | null>(null);
  let killing = $state<ContainerInfo | null>(null);
  let resDetails = $state<{ name: string; data: ResourceDetails | null; error: string; tab: "summary" | "raw" } | null>(null);
  let pruning = $state<PruneKind | null>(null);
  let pullText = $state("");
  let pulling = $state(false);
  let stateFilter = $state<"all" | "running" | "stopped">("all");

  function say(text: string) {
    note = text;
    setTimeout(() => {
      if (note === text) note = "";
    }, 4000);
  }

  function openMenu(e: MouseEvent, next: Omit<Menu, "x" | "y">) {
    e.preventDefault();
    e.stopPropagation();
    menu = { ...next, x: Math.min(e.clientX, window.innerWidth - 240), y: Math.min(e.clientY, window.innerHeight - 330) };
  }

  function closeMenu() {
    menu = null;
  }

  function copy(text: string, what: string) {
    navigator.clipboard.writeText(text).then(
      () => say(`${what} copied.`),
      () => say("Couldn't copy to the clipboard."),
    );
  }

  /** How long ago, in rough words. */
  function ago(seconds: number): string {
    const s = Math.max(0, Date.now() / 1000 - seconds);
    const minutes = s / 60;
    const hours = minutes / 60;
    const days = hours / 24;
    if (s < 90) return "just now";
    if (minutes < 90) return `${Math.round(minutes)} min ago`;
    if (hours < 36) return `${Math.round(hours)} h ago`;
    if (days < 45) return `${Math.round(days)} days ago`;
    if (days < 540) return `${Math.round(days / 30)} months ago`;
    return `${Math.round(days / 365)} years ago`;
  }

  /** Starts, stops or restarts several containers one after the other. */
  async function actAll(items: ContainerInfo[], action: "start" | "stop" | "restart") {
    const id = dockerId;
    if (!id) return;
    for (const c of items) {
      if (action === "start" ? c.state === "running" : c.state !== "running") continue;
      busy[c.id] = true;
      try {
        await dockerAction(id, c.id, action);
        error = "";
      } catch (err) {
        error = (err as { message?: string }).message ?? "That didn't work.";
      } finally {
        delete busy[c.id];
      }
    }
    await refresh();
  }

  async function confirmKill() {
    const target = killing;
    killing = null;
    if (target) await act(target, "kill");
  }

  async function confirmRename() {
    const r = renaming;
    const id = dockerId;
    if (!r || !id) return;
    try {
      await dockerRename(id, r.c.id, r.value.trim());
      renaming = null;
      await refresh();
    } catch (err) {
      r.error = (err as { message?: string }).message ?? "That didn't work.";
    }
  }

  async function showResourceDetails(r: DockerResource) {
    const id = dockerId;
    if (!id || view === "containers") return;
    resDetails = { name: r.name, data: null, error: "", tab: "summary" };
    try {
      const data = await dockerResourceDetails(id, view, r.id);
      if (resDetails) resDetails = { ...resDetails, data };
    } catch (err) {
      if (resDetails) resDetails = { ...resDetails, error: (err as { message?: string }).message ?? "Couldn't read the details." };
    }
  }

  const pruneWords: Record<PruneKind, string> = {
    containers: "every stopped container",
    images: "every untagged image that no container uses",
    volumes: "every volume that no container uses",
    networks: "every network that nothing is attached to",
  };

  async function confirmPrune() {
    const kind = pruning;
    const id = dockerId;
    pruning = null;
    if (!kind || !id) return;
    try {
      const done = await dockerPrune(id, kind);
      error = "";
      say(done.removed === 0 && done.bytes === 0 ? "Nothing to clear." : `Cleared ${done.removed}${done.bytes ? ` · freed ${size(done.bytes)}` : ""}.`);
    } catch (err) {
      error = (err as { message?: string }).message ?? "That did not work.";
    }
    await refresh();
  }

  let pullProgress = $state<PullProgress | null>(null);

  async function pullImage() {
    const id = dockerId;
    const reference = pullText.trim();
    if (!id || !reference || pulling) return;
    pulling = true;
    pullProgress = null;
    error = "";
    try {
      await dockerPull(id, reference, (p) => (pullProgress = p));
      say(`Downloaded ${reference}.`);
      pullText = "";
      await loadResources();
    } catch (err) {
      error = (err as { message?: string }).message ?? "The download did not work.";
    } finally {
      pulling = false;
      pullProgress = null;
    }
  }

  const pullPercent = $derived(pullProgress && pullProgress.total > 0 ? Math.min(100, Math.round((pullProgress.done / pullProgress.total) * 100)) : null);

  // ---- Running a new container.
  type RunForm = {
    image: string;
    name: string;
    restart: RunSpec["restart"];
    localhostOnly: boolean;
    ports: { host: string; container: string; udp: boolean }[];
    volumes: { source: string; target: string; readOnly: boolean }[];
    env: string;
    error: string;
    working: boolean;
  };
  let runForm = $state<RunForm | null>(null);

  function startRun(image = "") {
    runForm = { image, name: "", restart: "no", localhostOnly: true, ports: [], volumes: [], env: "", error: "", working: false };
  }

  async function confirmRun() {
    const form = runForm;
    const id = dockerId;
    if (!form || !id) return;
    const number = (text: string) => (/^\d{1,5}$/.test(text.trim()) ? Number(text.trim()) : NaN);
    const ports: RunSpec["ports"] = [];
    for (const p of form.ports) {
      if (!p.host.trim() && !p.container.trim()) continue;
      const [host, container] = [number(p.host), number(p.container)];
      if (!(host >= 1 && host <= 65535 && container >= 1 && container <= 65535)) {
        form.error = "Ports are numbers from 1 to 65535. Fill in both, or remove the row.";
        return;
      }
      ports.push({ host, container, udp: p.udp });
    }
    const volumes = form.volumes
      .filter((v) => v.source.trim() || v.target.trim())
      .map((v) => ({ source: v.source.trim(), target: v.target.trim(), read_only: v.readOnly }));
    const env = form.env
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
    form.error = "";
    form.working = true;
    try {
      await dockerRun(id, {
        image: form.image.trim(),
        name: form.name.trim(),
        ports,
        localhost_only: form.localhostOnly,
        volumes,
        env,
        restart: form.restart,
      });
      const started = form.name.trim() || form.image.trim();
      runForm = null;
      show("containers");
      say(`Started ${started}.`);
      await refresh();
    } catch (err) {
      form.error = (err as { message?: string }).message ?? "That didn't work.";
      form.working = false;
    }
  }

  // ---- Recent readings, for the small graphs.
  const HISTORY = 60;
  let history = $state<Record<string, { cpu: number[]; mem: number[] }>>({});

  function remember(list: ContainerStats[]) {
    const next: Record<string, { cpu: number[]; mem: number[] }> = {};
    for (const s of list) {
      const old = history[s.id] ?? { cpu: [], mem: [] };
      next[s.id] = { cpu: [...old.cpu, s.cpu_percent].slice(-HISTORY), mem: [...old.mem, s.memory_used].slice(-HISTORY) };
    }
    history = next;
  }

  async function connect() {
    await disconnect();
    connecting = true;
    error = "";
    operation = crypto.randomUUID();
    if (!wanted) {
      connecting = false;
      return;
    }
    shownName = wanted === LOCAL ? "This computer" : (active?.name ?? "");
    const target =
      wanted === LOCAL ? ({ type: "local" } as const) : ({ type: "ssh", saved_session_id: wanted } as const);
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
    resources = [];
    details = null;
    if (id) await dockerClose(id).catch(() => {});
  }

  // Following the active tab: a different server means a different connection.
  let followedId = "";
  $effect(() => {
    const id = active?.id;
    // The object changes when the tab's title does; only a different server matters.
    if (!id || id === followedId) return;
    followedId = id;
    untrack(() => {
      useLocal = false;
      void connect();
    });
  });

  function useThisComputer() {
    useLocal = true;
    followedId = "";
    void connect();
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
      if (view !== "containers") await loadResources();
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

  // Readings take a second or two per container, so they have their own, slower, loop.
  $effect(() => {
    const id = dockerId;
    if (!id) {
      stats = {};
      history = {};
      return;
    }
    let stopped = false;
    const read = async () => {
      if (document.hidden) return;
      try {
        const list = await dockerStats(id);
        if (!stopped) {
          stats = Object.fromEntries(list.map((s) => [s.id, s]));
          remember(list);
        }
      } catch {
        // The list shows the connection problem; readings are only extra.
      }
    };
    void read();
    const timer = setInterval(read, 6000);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  });

  function size(bytes: number): string {
    const units = ["B", "KB", "MB", "GB", "TB"];
    let n = bytes;
    let i = 0;
    while (n >= 1024 && i < units.length - 1) {
      n /= 1024;
      i++;
    }
    return `${n >= 10 || i === 0 ? Math.round(n) : n.toFixed(1)} ${units[i]}`;
  }

  function usage(s: ContainerStats | undefined): string {
    if (!s) return "";
    const memory = s.memory_limit > 0 ? `${size(s.memory_used)} of ${size(s.memory_limit)}` : size(s.memory_used);
    return `CPU ${s.cpu_percent.toFixed(1)}% · ${memory}`;
  }

  onDestroy(() => void disconnect());

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
    details = { id: container.id, name: container.name, data: null, error: "", tab: "summary" };
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
    const byState = containers.filter(
      (c) => stateFilter === "all" || (stateFilter === "running") === (c.state === "running"),
    );
    return word
      ? byState.filter((c) => `${c.name} ${c.image} ${c.project ?? ""}`.toLowerCase().includes(word))
      : byState;
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
      .map(([name, items]) => ({ name, items, project: true }));
    if (others.length) out.push({ name: out.length ? "Other containers" : "", items: others, project: false });
    return out;
  });

  const running = $derived(containers.filter((c) => c.state === "running").length);

  // What the running containers use all together.
  const totals = $derived.by(() => {
    const all = Object.values(stats);
    if (all.length === 0) return "";
    const cpu = all.reduce((sum, s) => sum + s.cpu_percent, 0);
    const memory = all.reduce((sum, s) => sum + s.memory_used, 0);
    return ` · CPU ${cpu.toFixed(0)}% · ${size(memory)}`;
  });

  function unhealthy(c: ContainerInfo): boolean {
    return c.status.includes("unhealthy");
  }

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
    ["usage", "Usage"],
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

<svelte:window onpointerdown={closeMenu} onblur={closeMenu} onkeydown={(e) => e.key === "Escape" && closeMenu()} />

<aside class="panel" aria-label="Docker">
  <header>
    <span class="heading"><Icon name="box" size={16} /> Docker{#if shownName}<span class="where">{shownName}</span>{/if}</span>
    <div class="head-tools">
      {#if !useLocal && !connecting}
        <button class="btn small" title="Use the Docker on this computer instead" onclick={useThisComputer}>
          This computer
        </button>
      {/if}
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

  {#if connecting}
    <div class="source"><button class="btn small" onclick={cancelConnecting}>Cancel</button></div>
  {/if}

  {#if error}
    <p class="message error">{error}</p>
    {#if wanted && !connecting && !dockerId}<p class="message"><button class="btn small" onclick={connect}>Try again</button></p>{/if}
  {/if}

  {#if connecting}
    <p class="hint">Connecting…</p>
  {:else if dockerId}
    <div class="views" role="tablist">
      {#each [["containers", "Containers"], ["images", "Images"], ["volumes", "Volumes"], ["networks", "Networks"]] as [id, label] (id)}
        <button
          role="tab"
          class="tab"
          class:on={view === id}
          aria-selected={view === id}
          onclick={() => show(id as "containers" | ResourceKind)}>{label}</button
        >
      {/each}
    </div>
    <div class="bar">
      <input type="search" placeholder="Filter" bind:value={filter} aria-label="Filter" />
      {#if view === "containers"}
        <select class="state-pick" bind:value={stateFilter} aria-label="Show">
          <option value="all">All</option>
          <option value="running">Running</option>
          <option value="stopped">Stopped</option>
        </select>
      {/if}
    </div>
    <div class="bar">
      {#if view === "containers"}<span class="count">{running} of {containers.length} running{totals}</span>{/if}
      {#if view === "images"}
        <input
          type="text"
          class="pull"
          placeholder="Download an image, such as nginx:latest"
          bind:value={pullText}
          disabled={pulling}
          aria-label="Image to download"
          onkeydown={(e) => e.key === "Enter" && pullImage()}
        />
        <button class="btn small" disabled={pulling || !pullText.trim()} onclick={pullImage}>{pulling ? "Downloading…" : "Pull"}</button>
      {/if}
      {#if view === "containers" || view === "images"}
        <button class="btn small" title="Start a new container" onclick={() => startRun()}>Run…</button>
      {/if}
      <span class="spacer"></span>
      <button class="btn small" title="Remove what is not in use" onclick={() => (pruning = view)}>Clean up…</button>
    </div>
    {#if pulling}
      <div class="progress" role="progressbar" aria-valuenow={pullPercent ?? undefined} aria-valuemin="0" aria-valuemax="100">
        <div class="fill" style:width="{pullPercent ?? 4}%"></div>
      </div>
      <p class="message note">
        {#if pullProgress && pullProgress.total > 0}{pullPercent}% · {size(pullProgress.done)} of {size(pullProgress.total)} · {pullProgress.status}{:else}Starting the download…{/if}
      </p>
    {/if}
    {#if note}<p class="message note">{note}</p>{/if}
    {#if view !== "containers"}
      <div class="list">
        {#each shownResources as r (r.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="row" ondblclick={() => showResourceDetails(r)} oncontextmenu={(e) => openMenu(e, { kind: "resource", r })}>
            <div class="what">
              <span class="name">{r.name}</span>
              <span class="sub">{r.detail}{r.created ? ` · ${ago(r.created)}` : ""}</span>
            </div>
            <div class="actions">
              {#if view === "images"}
                <button class="icon-btn small" title="Run a container from this image" onclick={() => startRun(r.name === "(untagged)" ? r.id : r.name)}>
                  <Icon name="play" size={13} />
                </button>
              {/if}
              <button class="icon-btn small" title="Details" onclick={() => showResourceDetails(r)}>
                <Icon name="info" size={13} />
              </button>
              {#if !r.protected}
                <button class="icon-btn small" title="Remove" onclick={() => (removingResource = r)}>
                  <Icon name="trash" size={13} />
                </button>
              {/if}
            </div>
          </div>
        {:else}
          <p class="hint">Nothing here.</p>
        {/each}
      </div>
    {:else}
    <div class="list">
      {#each groups as group (group.name)}
        {#if group.name}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="group" oncontextmenu={(e) => group.project && openMenu(e, { kind: "project", project: group.name, items: group.items })}>
            <h3>{group.name}</h3>
            {#if group.project}
              <span class="actions">
                <button class="icon-btn small" title="Start all" onclick={() => actAll(group.items, "start")}><Icon name="play" size={13} /></button>
                <button class="icon-btn small" title="Restart all" onclick={() => actAll(group.items, "restart")}><Icon name="refresh" size={13} /></button>
                <button class="icon-btn small" title="Stop all" onclick={() => actAll(group.items, "stop")}><Icon name="stop" size={13} /></button>
              </span>
            {/if}
          </div>
        {/if}
        {#each group.items as c (c.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="row" class:dim={c.state !== "running"} ondblclick={() => showDetails(c)} oncontextmenu={(e) => openMenu(e, { kind: "container", c })}>
            <span class="dot {unhealthy(c) ? 'bad' : dot(c.state)}" title={unhealthy(c) ? "unhealthy" : c.state}></span>
            <div class="what">
              <span class="name">{c.name}</span>
              <span class="sub">{c.image} · {c.status}{c.created ? ` · made ${ago(c.created)}` : ""}</span>
              {#if stats[c.id]}
                <span class="sub usage">
                  {usage(stats[c.id])}
                  {#if (history[c.id]?.cpu.length ?? 0) > 1}<Spark values={history[c.id].cpu} label="Recent CPU use" />{/if}
                </span>
              {/if}
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
    {/if}
  {:else if !error}
    <p class="hint">
      Open an SSH tab and its Docker appears here by itself. Nothing is installed on the server; its user
      needs access to Docker.
    </p>
  {/if}

  {#if question}
    <PromptDialog prompt={question.prompt} onanswer={answer} />
  {/if}

  {#if menu}
    <ul class="menu ctx" role="menu" style:left="{menu.x}px" style:top="{menu.y}px" onpointerdown={(e) => e.stopPropagation()} oncontextmenu={(e) => e.preventDefault()}>
      {#if menu.kind === "container" && menu.c}
        {@const c = menu.c}
        {#if c.state === "running"}
          <li><button onclick={() => { closeMenu(); open("docker_shell", c); }}><Icon name="terminal" size={14} /> Open a shell</button></li>
        {/if}
        <li><button onclick={() => { closeMenu(); open("docker_logs", c); }}><Icon name="log" size={14} /> Show the log</button></li>
        <li><button onclick={() => { closeMenu(); void showDetails(c); }}><Icon name="info" size={14} /> Details</button></li>
        <li class="sep" role="separator"></li>
        {#if c.state === "running"}
          <li><button onclick={() => { closeMenu(); void act(c, "stop"); }}><Icon name="stop" size={14} /> Stop</button></li>
          <li><button onclick={() => { closeMenu(); void act(c, "restart"); }}><Icon name="refresh" size={14} /> Restart</button></li>
          <li><button onclick={() => { closeMenu(); void act(c, "pause"); }}><Icon name="stop" size={14} /> Pause</button></li>
          <li><button onclick={() => { closeMenu(); killing = c; }}><Icon name="close" size={14} /> Kill…</button></li>
        {:else if c.state === "paused"}
          <li><button onclick={() => { closeMenu(); void act(c, "unpause"); }}><Icon name="play" size={14} /> Resume</button></li>
        {:else}
          <li><button onclick={() => { closeMenu(); void act(c, "start"); }}><Icon name="play" size={14} /> Start</button></li>
        {/if}
        <li><button onclick={() => { closeMenu(); renaming = { c, value: c.name, error: "" }; }}><Icon name="edit" size={14} /> Rename…</button></li>
        <li class="sep" role="separator"></li>
        <li><button onclick={() => { closeMenu(); copy(c.name, "Name"); }}><Icon name="copy" size={14} /> Copy name</button></li>
        <li><button onclick={() => { closeMenu(); copy(c.id, "ID"); }}><Icon name="copy" size={14} /> Copy ID</button></li>
        {#if c.state === "running"}
          <li><button onclick={() => { closeMenu(); copy(`docker exec -it ${c.name} sh`, "Command"); }}><Icon name="copy" size={14} /> Copy the shell command</button></li>
        {/if}
        <li class="sep" role="separator"></li>
        <li><button onclick={() => { closeMenu(); removing = c; }}><Icon name="trash" size={14} /> Remove…</button></li>
      {:else if menu.kind === "project" && menu.items}
        {@const items = menu.items}
        <li><button onclick={() => { closeMenu(); void actAll(items, "start"); }}><Icon name="play" size={14} /> Start all</button></li>
        <li><button onclick={() => { closeMenu(); void actAll(items, "restart"); }}><Icon name="refresh" size={14} /> Restart all</button></li>
        <li><button onclick={() => { closeMenu(); void actAll(items, "stop"); }}><Icon name="stop" size={14} /> Stop all</button></li>
        <li class="sep" role="separator"></li>
        <li><button onclick={() => { closeMenu(); copy(menu?.project ?? "", "Name"); }}><Icon name="copy" size={14} /> Copy project name</button></li>
      {:else if menu.kind === "resource" && menu.r}
        {@const r = menu.r}
        {#if view === "images"}
          <li><button onclick={() => { closeMenu(); startRun(r.name === "(untagged)" ? r.id : r.name); }}><Icon name="play" size={14} /> Run a container…</button></li>
        {/if}
        <li><button onclick={() => { closeMenu(); void showResourceDetails(r); }}><Icon name="info" size={14} /> Details</button></li>
        <li><button onclick={() => { closeMenu(); copy(r.name, "Name"); }}><Icon name="copy" size={14} /> Copy name</button></li>
        <li><button onclick={() => { closeMenu(); copy(r.id, "ID"); }}><Icon name="copy" size={14} /> Copy ID</button></li>
        {#if !r.protected}
          <li class="sep" role="separator"></li>
          <li><button onclick={() => { closeMenu(); removingResource = r; }}><Icon name="trash" size={14} /> Remove…</button></li>
        {/if}
      {/if}
    </ul>
  {/if}

  {#if runForm}
    <div class="modal-backdrop">
      <div class="modal wide" role="dialog" aria-label="Run a container" tabindex="-1" onkeydown={(e) => e.key === "Escape" && !runForm?.working && (runForm = null)}>
        <h2>Run a container</h2>
        <label class="field">
          Image
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" autofocus bind:value={runForm.image} placeholder="nginx:latest" spellcheck="false" />
          <small>If it isn't on this computer yet, it is downloaded first.</small>
        </label>
        <div class="two">
          <label class="field">
            Name (optional)
            <input type="text" bind:value={runForm.name} spellcheck="false" />
          </label>
          <label class="field">
            Restart
            <select bind:value={runForm.restart}>
              <option value="no">Never</option>
              <option value="unless-stopped">Unless stopped</option>
              <option value="always">Always</option>
              <option value="on-failure">On failure</option>
            </select>
          </label>
        </div>

        <h3 class="sect">Ports <button class="btn small" onclick={() => runForm?.ports.push({ host: "", container: "", udp: false })}>Add</button></h3>
        {#each runForm.ports as p, i (i)}
          <div class="pair">
            <input type="text" inputmode="numeric" placeholder="Computer port" bind:value={p.host} aria-label="Port on the computer" />
            <span>→</span>
            <input type="text" inputmode="numeric" placeholder="Container port" bind:value={p.container} aria-label="Port in the container" />
            <label class="check"><input type="checkbox" bind:checked={p.udp} /> UDP</label>
            <button class="icon-btn small" title="Remove" onclick={() => runForm?.ports.splice(i, 1)}><Icon name="close" size={13} /></button>
          </div>
        {/each}
        {#if runForm.ports.length}
          <label class="check"><input type="checkbox" bind:checked={runForm.localhostOnly} /> Reachable from this computer only (127.0.0.1)</label>
        {/if}

        <h3 class="sect">Folders and volumes <button class="btn small" onclick={() => runForm?.volumes.push({ source: "", target: "", readOnly: false })}>Add</button></h3>
        {#each runForm.volumes as v, i (i)}
          <div class="pair">
            <input type="text" placeholder="Folder (/srv/data) or volume name" bind:value={v.source} spellcheck="false" aria-label="Folder or volume" />
            <span>→</span>
            <input type="text" placeholder="Path in the container" bind:value={v.target} spellcheck="false" aria-label="Path in the container" />
            <label class="check"><input type="checkbox" bind:checked={v.readOnly} /> Read-only</label>
            <button class="icon-btn small" title="Remove" onclick={() => runForm?.volumes.splice(i, 1)}><Icon name="close" size={13} /></button>
          </div>
        {/each}

        <label class="field">
          Environment variables (one NAME=value per line)
          <textarea rows="3" bind:value={runForm.env} spellcheck="false"></textarea>
        </label>
        {#if runForm.error}<p class="message error">{runForm.error}</p>{/if}
        <div class="modal-actions">
          <button class="btn" disabled={runForm.working} onclick={() => (runForm = null)}>Cancel</button>
          <button class="btn primary" disabled={runForm.working || !runForm.image.trim()} onclick={confirmRun}>
            {runForm.working ? "Starting…" : "Run"}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if killing}
    <div class="modal-backdrop">
      <div class="modal" role="alertdialog" aria-label="Kill container" tabindex="-1">
        <h2>Kill “{killing.name}”?</h2>
        <p>The container is ended at once, without a chance to shut down cleanly. Unsaved work in it may be lost.</p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (killing = null)}>Cancel</button>
          <button class="btn danger" onclick={confirmKill}>Kill</button>
        </div>
      </div>
    </div>
  {/if}

  {#if renaming}
    <div class="modal-backdrop">
      <div class="modal" role="dialog" aria-label="Rename container" tabindex="-1" onkeydown={(e) => e.key === "Escape" && (renaming = null)}>
        <h2>Rename container</h2>
        <label class="field">
          New name for {renaming.c.name}
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" autofocus bind:value={renaming.value} onkeydown={(e) => e.key === "Enter" && confirmRename()} />
        </label>
        {#if renaming.error}<p class="message error">{renaming.error}</p>{/if}
        <div class="modal-actions">
          <button class="btn" onclick={() => (renaming = null)}>Cancel</button>
          <button class="btn primary" onclick={confirmRename}>Rename</button>
        </div>
      </div>
    </div>
  {/if}

  {#if pruning}
    <div class="modal-backdrop">
      <div class="modal" role="alertdialog" aria-label="Clean up" tabindex="-1">
        <h2>Clean up {pruning}?</h2>
        <p>This removes {pruneWords[pruning]}. It can't be undone.</p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (pruning = null)}>Cancel</button>
          <button class="btn danger" onclick={confirmPrune}>Remove</button>
        </div>
      </div>
    </div>
  {/if}

  {#if resDetails}
    {@const shown = resDetails.data ? (resDetails.tab === "raw" ? resDetails.data.raw : resDetails.data.lines.join("\n") || "Nothing to show.") : ""}
    <div class="modal-backdrop">
      <div class="modal wide" role="dialog" aria-label="Details" tabindex="-1">
        <h2>{resDetails.name}</h2>
        {#if resDetails.error}
          <p class="message error">{resDetails.error}</p>
        {:else if !resDetails.data}
          <p class="hint">Reading…</p>
        {:else}
          <div class="tabs" role="tablist">
            {#each [["summary", "Summary"], ["raw", "Raw"]] as [id, label] (id)}
              <button
                role="tab"
                class="tab"
                class:on={resDetails.tab === id}
                aria-selected={resDetails.tab === id}
                onclick={() => resDetails && (resDetails = { ...resDetails, tab: id as "summary" | "raw" })}>{label}</button
              >
            {/each}
          </div>
          <pre>{shown}</pre>
        {/if}
        <div class="modal-actions">
          {#if resDetails.data}
            <button class="btn" onclick={() => copy(shown, "Text")}>Copy</button>
          {/if}
          <button class="btn" onclick={() => (resDetails = null)}>Close</button>
        </div>
      </div>
    </div>
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

  {#if removingResource}
    <div class="modal-backdrop">
      <div class="modal" role="alertdialog" aria-label="Remove" tabindex="-1">
        <h2>Remove “{removingResource.name}”?</h2>
        <p>Docker refuses if anything still uses it.</p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (removingResource = null)}>Cancel</button>
          <button class="btn danger" onclick={confirmRemoveResource}>Remove</button>
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
          {#if details.tab === "usage"}
            {@const h = history[details.id]}
            {#if h && h.cpu.length > 1}
              <div class="graphs">
                <div>
                  <span class="sub">CPU, last {Math.round((h.cpu.length * 6) / 60)} min · now {h.cpu[h.cpu.length - 1].toFixed(1)}% · highest {Math.max(...h.cpu).toFixed(1)}%</span>
                  <Spark values={h.cpu} width={420} height={70} label="CPU use" />
                </div>
                <div>
                  <span class="sub">Memory · now {size(h.mem[h.mem.length - 1])} · highest {size(Math.max(...h.mem))}</span>
                  <Spark values={h.mem} width={420} height={70} label="Memory use" />
                </div>
              </div>
            {:else}
              <p class="hint">Readings appear here while the container is running. Keep this open for a few seconds.</p>
            {/if}
          {:else}
            <pre>{lines(details.data, details.tab)}</pre>
          {/if}
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
    border-top: 1px solid var(--border);
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
  .where {
    max-width: 150px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 500;
  }
  .source {
    display: flex;
    gap: 6px;
    margin: 6px 12px;
  }
    .views {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 2px 12px 6px;
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
  .progress {
    height: 6px;
    margin: 2px 14px 4px;
    overflow: hidden;
    background: var(--bg-2);
    border-radius: 3px;
  }
  .progress .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .usage {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .graphs {
    display: grid;
    gap: 12px;
    margin-bottom: 10px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .sect {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 12px 0 6px;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 650;
  }
  .pair {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
  }
  .pair input[type="text"] {
    flex: 1;
    min-width: 0;
  }
  .dot.bad {
    background: var(--danger, #f85149);
  }
  .spacer {
    flex: 1;
  }
  .state-pick {
    flex: none;
    width: auto;
  }
  .note {
    color: var(--accent);
  }
  .group {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 8px;
  }
  .group .actions {
    opacity: 0.55;
  }
  .group:hover .actions {
    opacity: 1;
  }
  .ctx {
    position: fixed;
    z-index: 60;
    min-width: 220px;
  }
  .ctx li {
    list-style: none;
  }
  .ctx .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
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
