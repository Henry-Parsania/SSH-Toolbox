<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { promptRespond, type Prompt, type PromptResponse } from "../backend";
  import {
    filesClose,
    filesOpenLocal,
    filesOpenSftp,
    filesOpenWsl,
    onDesktopFileDrop,
    transferAdd,
    transfersSubscribe,
    type OpenedFiles,
    type TransferJob,
  } from "../backend/files";
  import PromptDialog from "../terminal/PromptDialog.svelte";
  import Icon from "../ui/Icon.svelte";
  import FileBrowser from "./FileBrowser.svelte";
  import TransferList from "./TransferList.svelte";

  interface Props {
    /** Whose files to show beside this computer's: a saved SSH session or a WSL distribution. */
    source: { kind: "ssh"; id: string; name: string } | { kind: "wsl"; distro: string; name: string } | null;
    onclose: () => void;
  }

  let { source, onclose }: Props = $props();

  let local = $state<OpenedFiles | null>(null);
  let remote = $state<OpenedFiles | null>(null);
  let localPath = $state("");
  let remotePath = $state("");
  let remoteStatus = $state("");
  let message = $state("");
  let jobs = $state<TransferJob[]>([]);
  let question = $state<{ id: string; prompt: Prompt } | null>(null);

  onMount(async () => {
    try {
      await transfersSubscribe((list) => (jobs = list));
      local = await filesOpenLocal();
    } catch (err) {
      message = (err as { message?: string }).message ?? "Couldn't open your files.";
    }
  });

  // The server side follows the active pane: a different server means a different list.
  let openedFor: string | null = null;
  $effect(() => {
    const wanted = source ? (source.kind === "ssh" ? `ssh:${source.id}` : `wsl:${source.distro}`) : null;
    if (wanted === openedFor) return;
    openedFor = wanted;
    const old = remote;
    remote = null;
    remotePath = "";
    if (old) void filesClose(old.id).catch(() => {});
    if (!wanted) {
      remoteStatus = "";
      return;
    }
    remoteStatus = "Connecting…";
    const operation = crypto.randomUUID();
    const opening =
      source?.kind === "wsl"
        ? filesOpenWsl(source.distro)
        : filesOpenSftp(operation, source!.kind === "ssh" ? source!.id : "", (status) => {
            if (status.type === "prompt") question = { id: status.prompt_id, prompt: status.prompt };
          });
    opening.then(
      (opened) => {
        question = null;
        if (openedFor === wanted) remote = opened;
        else void filesClose(opened.id).catch(() => {});
      },
      (err: { message?: string }) => {
        question = null;
        if (openedFor === wanted) remoteStatus = err.message ?? "Couldn't open the server's files.";
      },
    );
  });

  onDestroy(() => {
    if (local) void filesClose(local.id).catch(() => {});
    if (remote) void filesClose(remote.id).catch(() => {});
  });

  function answer(response: PromptResponse) {
    if (!question) return;
    void promptRespond(question.id, response).catch(() => {});
    question = null;
  }

  // ---- Dragging rows from one list to the other, and files dropped from the desktop.
  let browsers: Record<"remote" | "local", { transferSelection: () => void } | undefined> = $state({
    remote: undefined,
    local: undefined,
  });
  let drag = $state<{ from: "remote" | "local"; count: number; x: number; y: number } | null>(null);
  let desktopOver = $state(false);

  function sideAt(x: number, y: number): "remote" | "local" | null {
    const el = document.elementFromPoint(x, y)?.closest("[data-side]");
    const side = el?.getAttribute("data-side");
    return side === "remote" || side === "local" ? side : null;
  }

  function startDrag(from: "remote" | "local", count: number) {
    drag = { from, count, x: -100, y: -100 };
    const move = (e: PointerEvent) => {
      if (drag) drag = { ...drag, x: e.clientX, y: e.clientY };
    };
    const up = (e: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const over = sideAt(e.clientX, e.clientY);
      const started = drag?.from;
      drag = null;
      if (started && over && over !== started) browsers[started]?.transferSelection();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  let unlisten: (() => void) | undefined;
  onMount(() => {
    onDesktopFileDrop({
      hover: (x, y) => (desktopOver = sideAt(x, y) === "remote" && !!remote),
      leave: () => (desktopOver = false),
      drop: (paths, x, y) => {
        desktopOver = false;
        // Files from the desktop go up to the server folder shown, when dropped on that list.
        if (local && remote && sideAt(x, y) === "remote") {
          for (const path of paths) void queue(local.id, [path], remote.id, remotePath);
        }
      },
    }).then((stop) => (unlisten = stop), () => {});
  });
  onDestroy(() => unlisten?.());

  async function queue(fromId: string, paths: string[], toId: string, toDir: string) {
    message = "";
    for (const path of paths) {
      try {
        const added = await transferAdd(fromId, path, toId, toDir);
        if (added.skipped.length) {
          message = `Left out: ${added.skipped.slice(0, 3).join("; ")}${added.skipped.length > 3 ? "…" : ""}`;
        }
      } catch (err) {
        message = (err as { message?: string }).message ?? "Couldn't start that transfer.";
      }
    }
  }

  // Both lists can be refreshed from outside by changing their path to the same value, but a
  // finished transfer is shown by the next refresh, so the lists reload when the queue empties.
  let wasBusy = false;
  let refresh = $state(0);
  $effect(() => {
    const busy = jobs.some((j) => ["queued", "asking", "running"].includes(j.state.state));
    if (wasBusy && !busy) refresh++;
    wasBusy = busy;
  });
</script>

<aside class="panel" aria-label="Files">
  <header>
    <span class="heading"><Icon name="folder" size={15} /> Files</span>
    <button class="icon-btn" title="Close the file panel" onclick={onclose}><Icon name="close" size={15} /></button>
  </header>

  {#if message}<p class="message warn top">{message}</p>{/if}

  <div class="browsers">
    <FileBrowser
        title={source ? source.name : "Server"}
        side="remote"
        bind:this={browsers.remote}
        highlight={desktopOver || (drag !== null && drag.from === "local")}
        onstartdrag={(count) => startDrag("remote", count)}
        reloadSignal={refresh}
        files={remote}
        status={source ? remoteStatus : "Open an SSH or WSL session in the active pane to see its files here."}
        bind:path={remotePath}
        transferLabel="Download"
        transferIcon="down"
        ontransfer={(paths) => local && remote && queue(remote.id, paths, local.id, localPath)}
      />
    <div class="divider"></div>
    <FileBrowser
        title="This computer"
        side="local"
        bind:this={browsers.local}
        highlight={drag !== null && drag.from === "remote"}
        onstartdrag={(count) => startDrag("local", count)}
        reloadSignal={refresh}
        files={local}
        status="Opening…"
        bind:path={localPath}
        transferLabel="Upload"
        transferIcon="up"
        ontransfer={(paths) => local && remote && queue(local.id, paths, remote.id, remotePath)}
      />
  </div>

  <TransferList {jobs} />

  {#if drag}
    <div class="ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 12}px">
      <Icon name={drag.from === "remote" ? "down" : "up"} size={13} />
      {drag.count} item{drag.count === 1 ? "" : "s"}
    </div>
  {/if}

  {#if question}
    <PromptDialog prompt={question.prompt} onanswer={answer} />
  {/if}
</aside>

<style>
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 420px;
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
  .top {
    margin: 4px 12px 0;
  }
  .browsers {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .ghost {
    position: fixed;
    z-index: 200;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    color: var(--on-accent);
    background: var(--accent);
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    pointer-events: none;
    box-shadow: var(--shadow-small);
  }
  .divider {
    height: 1px;
    margin: 2px 0;
    background: var(--border-strong);
  }
</style>
