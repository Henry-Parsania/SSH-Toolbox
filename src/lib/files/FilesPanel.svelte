<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { wslPath } from "../terminal/cwd";
  import { terminalWrite } from "../backend";
  import { shellQuote } from "./paths";
  import { promptRespond, type Prompt, type PromptResponse } from "../backend";
  import {
    filesClose,
    filesOpenFtp,
    filesOpenLocal,
    filesOpenSftp,
    filesOpenWsl,
    onDesktopFileDrop,
    openLocalFile,
    transferAdd,
    transfersSubscribe,
    type OpenedFiles,
    type TransferJob,
  } from "../backend/files";
  import PromptDialog from "../terminal/PromptDialog.svelte";
  import Icon from "../ui/Icon.svelte";
  import EditList from "./EditList.svelte";
  import { edits } from "./edits.svelte";
  import FileBrowser from "./FileBrowser.svelte";
  import TransferList from "./TransferList.svelte";

  interface Props {
    /** Whose files to show beside this computer's: a saved SSH session or a WSL distribution. */
    source:
      | { kind: "ssh"; id: string; name: string }
      | { kind: "ftp"; id: string; name: string }
      | { kind: "wsl"; distro: string; name: string }
      | null;
    /** The folder the active terminal reported, if any. */
    follow?: string;
    /** The backend session of the active terminal, so the panel can send it a folder to go to. */
    terminalSession?: string;
    onclose: () => void;
  }

  let { source, follow = "", terminalSession = "", onclose }: Props = $props();

  // Where the divider between the server's files and this computer's sits, as the top list's
  // share of the height. It can be dragged, and is kept for next time.
  let split = $state(
    (() => {
      const n = Number(localStorage.getItem("files-split"));
      return n >= 0.15 && n <= 0.85 ? n : 0.5;
    })(),
  );
  let browsersBox: HTMLElement | undefined = $state();

  function resizeSplit(e: PointerEvent) {
    if (!browsersBox) return;
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const box = browsersBox.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      split = Math.min(0.85, Math.max(0.15, (ev.clientY - box.top) / box.height));
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      localStorage.setItem("files-split", split.toFixed(3));
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  }

  let local = $state<OpenedFiles | null>(null);
  let remote = $state<OpenedFiles | null>(null);
  let localPath = $state("");
  let remotePath = $state("");
  let remoteStatus = $state("");
  let message = $state("");
  let jobs = $state<TransferJob[]>([]);
  let question = $state<{ id: string; prompt: Prompt } | null>(null);

  /** A file of the server opens here, and what is saved goes back. */
  async function openRemoteFile(_entry: unknown, path: string, choose: boolean) {
    if (!remote) return;
    message = "";
    try {
      await edits.open(remote.id, path, choose);
    } catch (err) {
      message = (err as { message?: string }).message ?? "Couldn't open that file.";
    }
  }

  async function openHereFile(_entry: unknown, path: string, choose: boolean) {
    message = "";
    try {
      await openLocalFile(path, choose);
    } catch (err) {
      message = (err as { message?: string }).message ?? "Couldn't open that file.";
    }
  }

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
    const wanted = source
      ? source.kind === "wsl"
        ? `wsl:${source.distro}`
        : `${source.kind}:${source.id}`
      : null;
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
    const ask = (status: { type: string; prompt_id?: string; prompt?: Prompt }) => {
      if (status.type === "prompt") question = { id: status.prompt_id!, prompt: status.prompt! };
    };
    const opening =
      source?.kind === "wsl"
        ? filesOpenWsl(source.distro)
        : source?.kind === "ftp"
          ? filesOpenFtp(operation, source.id, ask)
          : filesOpenSftp(operation, source!.kind === "ssh" ? source!.id : "", ask);
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
  // The server list can follow the terminal's folder. Only the list changes: nothing is
  // changed on the server, and the terminal is not touched.
  let following = $state(localStorage.getItem("files-follow") !== "off");
  function toggleFollow() {
    following = !following;
    localStorage.setItem("files-follow", following ? "on" : "off");
  }
  $effect(() => {
    if (!following || !follow || !remote || !source || source.kind === "ftp") return;
    const target = source.kind === "wsl" ? wslPath(source.distro, follow) : follow;
    if (target !== untrack(() => remotePath)) remotePath = target;
  });

  // ---- Moving the active SSH terminal to the folder shown here.
  const canMoveTerminal = $derived(source?.kind === "ssh" && !!terminalSession && !!remote && !!remotePath);
  let moved = $state("");

  async function moveTerminal() {
    if (!canMoveTerminal) return;
    // Ctrl+U clears whatever was half typed on the line, so the command stands alone.
    const line = `\x15cd -- ${shellQuote(remotePath)}\r`;
    try {
      await terminalWrite(terminalSession, new TextEncoder().encode(line));
      moved = "";
    } catch (err) {
      moved = (err as { message?: string }).message ?? "Couldn't send that to the terminal.";
    }
  }

  /** Types text at the terminal's prompt without pressing Enter. */
  async function typeAtPrompt(text: string) {
    try {
      await terminalWrite(terminalSession, new TextEncoder().encode(text));
      moved = "";
    } catch (err) {
      moved = (err as { message?: string }).message ?? "Couldn't send that to the terminal.";
    }
  }

  // Most shells don't say which folder they are in, so following needs a little help: this types
  // one line into the shell that makes it report its folder (bash and zsh).
  const REPORT_LINE =
    ` __osc7(){ printf '\\033]7;file://%s%s\\007' "$HOSTNAME" "$PWD"; }; if [ -n "$ZSH_VERSION" ]; then precmd_functions+=(__osc7); else PROMPT_COMMAND="__osc7\${PROMPT_COMMAND:+;$PROMPT_COMMAND}"; fi; __osc7`;
  async function setUpShell() {
    if (!terminalSession) return;
    try {
      await terminalWrite(terminalSession, new TextEncoder().encode(`\x15${REPORT_LINE}\r`));
      moved = "";
    } catch (err) {
      moved = (err as { message?: string }).message ?? "Couldn't send that to the terminal.";
    }
  }

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
    <span class="head-tools">
    <button class="icon-btn" title="Close the file panel" onclick={onclose}><Icon name="close" size={15} /></button>
    </span>
  </header>

  {#if message}<p class="message warn top">{message}</p>{/if}

  {#if source && source.kind !== "ftp"}
    <div class="options">
      <label class="check" title="The server list shows the folder the terminal is in">
        <input type="checkbox" checked={following} onchange={toggleFollow} />
        Follow the terminal's folder
      </label>
    </div>
    {#if following && !follow && source.kind === "ssh" && terminalSession}
      <p class="hint">
        This terminal hasn't reported its folder. Most shells don't until asked.
        <button class="btn small" onclick={setUpShell}>Set up this shell</button>
      </p>
    {/if}
    {#if moved}<p class="message warn top">{moved}</p>{/if}
  {/if}

  <div class="browsers" bind:this={browsersBox}>
    <div class="half" style:flex="{split} 1 0">
    <FileBrowser
        title={source ? source.name : "Server"}
        side="remote"
        bind:this={browsers.remote}
        highlight={desktopOver || (drag !== null && drag.from === "local")}
        onstartdrag={(count) => startDrag("remote", count)}
        reloadSignal={refresh}
        files={remote}
        oncopytoterminal={source?.kind === "ssh" && terminalSession ? typeAtPrompt : undefined}
        extra={source?.kind === "ssh"
          ? {
              icon: "terminal",
              title: "Go to this folder in the terminal (types a cd command)",
              disabled: !canMoveTerminal,
              onclick: moveTerminal,
            }
          : undefined}
        status={source ? remoteStatus : "Open an SSH or WSL session, or an FTP session from the list, to see its files here."}
        bind:path={remotePath}
        transferLabel="Download"
        transferIcon="down"
        ontransfer={(paths) => local && remote && queue(remote.id, paths, local.id, localPath)}
        onopenfile={openRemoteFile}
      />
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="divider" onpointerdown={resizeSplit} title="Drag to change the sizes"></div>
    <div class="half" style:flex="{1 - split} 1 0">
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
        onopenfile={openHereFile}
      />
    </div>
  </div>

  <EditList />

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
    width: 100%;
    flex: none;
    min-height: 0;
    background: var(--bg-1);
    border-left: 1px solid var(--border);
  }
  .head-tools {
    display: flex;
    gap: 2px;
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
  .options {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin: 2px 12px 6px;
    font-size: 12px;
  }
  .options .check {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .hint {
    margin: 0 12px 6px;
    color: var(--text-muted);
    font-size: 12px;
  }
  .half {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  /* A thin line with a taller strip around it, so it is easy to grab. */
  .divider {
    flex: none;
    position: relative;
    height: 1px;
    margin: 2px 0;
    background: var(--border-strong);
    cursor: row-resize;
  }
  .divider::before {
    content: "";
    position: absolute;
    inset: -4px 0;
  }
  .divider:hover {
    background: var(--accent);
  }
</style>
