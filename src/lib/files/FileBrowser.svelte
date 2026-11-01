<script lang="ts">
  import { untrack } from "svelte";
  import {
    filesChmod,
    filesDelete,
    filesDeletePlan,
    filesList,
    filesMkdir,
    filesRename,
    type FileEntry,
    type OpenedFiles,
  } from "../backend/files";
  import Icon, { type IconName } from "../ui/Icon.svelte";
  import { baseName, formatSize, formatTime, joinPath, modeToText, parentPath, parseMode, rootPath, sortEntries } from "./paths";

  interface Props {
    title: string;
    /** The open file backend, or none yet. */
    files: OpenedFiles | null;
    /** Shown instead of the list while this has text (connecting, or why there is nothing). */
    status?: string;
    /** What the transfer button does with the selected paths. */
    ontransfer?: (paths: string[]) => void;
    transferLabel?: string;
    transferIcon?: IconName;
    /** The folder being shown, so the other side knows where to put things. */
    path?: string;
    /** Reloads the folder each time this number goes up. */
    reloadSignal?: number;
    /** Which list this is, so a drag can tell where it was dropped. */
    side?: "remote" | "local";
    /** Marks the list as the place a drop would land. */
    highlight?: boolean;
    /** Called when rows are dragged away, with how many. */
    onstartdrag?: (count: number) => void;
  }

  let { title, files, status = "", ontransfer, transferLabel = "Transfer", transferIcon = "down", path = $bindable(""), reloadSignal = 0, side = "local", highlight = false, onstartdrag }: Props = $props();

  let entries = $state<FileEntry[]>([]);
  let loading = $state(false);
  let error = $state("");
  let selected = $state<string[]>([]);
  let anchor = $state<string | null>(null);
  let typed = $state("");
  let showHidden = $state(true);

  type Dialog =
    | { kind: "mkdir"; value: string }
    | { kind: "rename"; entry: FileEntry; value: string }
    | { kind: "delete"; entries: FileEntry[]; files: number; folders: number }
    | { kind: "chmod"; entry: FileEntry; value: string };
  let dialog = $state<Dialog | null>(null);
  let dialogError = $state("");
  let working = $state(false);

  const sep = $derived(files?.separator ?? "/");
  const visible = $derived(sortEntries(entries).filter((e) => showHidden || !e.name.startsWith(".")));
  const chosen = $derived(visible.filter((e) => selected.includes(e.name)));

  // Start in the home folder when a backend opens.
  $effect(() => {
    const home = files?.home;
    if (home) {
      if (!untrack(() => path)) path = home;
    } else {
      entries = [];
      path = "";
    }
  });

  $effect(() => {
    if (reloadSignal > 0) untrack(reload);
  });

  // Show whatever folder is chosen.
  $effect(() => {
    const id = files?.id;
    const here = path;
    if (id && here) untrack(() => void load(id, here));
  });

  $effect(() => {
    typed = path;
  });

  async function load(id: string, here: string) {
    loading = true;
    error = "";
    try {
      const found = await filesList(id, here);
      if (files?.id === id && path === here) {
        entries = found;
        selected = [];
        anchor = null;
      }
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't open that folder.";
      entries = [];
    } finally {
      loading = false;
    }
  }

  function reload() {
    if (files && path) void load(files.id, path);
  }

  function go(to: string) {
    if (to && to !== path) path = to;
    else reload();
  }

  function open(entry: FileEntry) {
    if (entry.kind === "dir" || entry.kind === "symlink") go(joinPath(path, entry.name, sep));
  }

  function pick(entry: FileEntry, e: MouseEvent) {
    if (e.shiftKey && anchor) {
      const names = visible.map((v) => v.name);
      const [a, b] = [names.indexOf(anchor), names.indexOf(entry.name)].sort((x, y) => x - y);
      selected = names.slice(a, b + 1);
    } else if (e.ctrlKey || e.metaKey) {
      selected = selected.includes(entry.name) ? selected.filter((n) => n !== entry.name) : [...selected, entry.name];
      anchor = entry.name;
    } else {
      selected = [entry.name];
      anchor = entry.name;
    }
  }

  function pathOf(entry: FileEntry): string {
    return joinPath(path, entry.name, sep);
  }

  function transfer() {
    if (ontransfer && chosen.length) ontransfer(chosen.map(pathOf));
  }

  /** Sends the selected rows across, as the button does. Used when they are dragged over. */
  export function transferSelection() {
    transfer();
  }

  // Pressing on a row and moving a little starts a drag of the selected rows.
  function rowPointerDown(entry: FileEntry, e: PointerEvent) {
    if (e.button !== 0 || !onstartdrag || e.shiftKey || e.ctrlKey || e.metaKey) return;
    const startX = e.clientX;
    const startY = e.clientY;
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
    const move = (ev: PointerEvent) => {
      if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < 6) return;
      stop();
      if (!selected.includes(entry.name)) {
        selected = [entry.name];
        anchor = entry.name;
      }
      onstartdrag?.(selected.length);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
  }

  async function startDelete() {
    if (!files || chosen.length === 0) return;
    dialogError = "";
    working = true;
    try {
      let fileCount = 0;
      let folderCount = 0;
      for (const entry of chosen) {
        const plan = await filesDeletePlan(files.id, pathOf(entry));
        fileCount += plan.files;
        folderCount += plan.folders;
      }
      dialog = { kind: "delete", entries: chosen, files: fileCount, folders: folderCount };
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't look inside that.";
    } finally {
      working = false;
    }
  }

  async function confirm() {
    const d = dialog;
    if (!d || !files) return;
    working = true;
    dialogError = "";
    try {
      if (d.kind === "mkdir") {
        const name = d.value.trim();
        if (!name || /[\\/]/.test(name)) throw { message: "Enter a name without / or \\." };
        await filesMkdir(files.id, joinPath(path, name, sep));
      } else if (d.kind === "rename") {
        const name = d.value.trim();
        if (!name || /[\\/]/.test(name)) throw { message: "Enter a name without / or \\." };
        await filesRename(files.id, pathOf(d.entry), joinPath(path, name, sep));
      } else if (d.kind === "delete") {
        for (const entry of d.entries) await filesDelete(files.id, pathOf(entry));
      } else if (d.kind === "chmod") {
        const mode = parseMode(d.value);
        if (mode === null) throw { message: "Use three or four digits from 0 to 7, such as 755." };
        await filesChmod(files.id, pathOf(d.entry), mode);
      }
      dialog = null;
      reload();
    } catch (err) {
      dialogError = (err as { message?: string }).message ?? "That didn't work.";
      if (d.kind === "delete") reload();
    } finally {
      working = false;
    }
  }

  function icon(entry: FileEntry): IconName {
    return entry.kind === "dir" ? "folder" : entry.kind === "symlink" ? "route" : "file";
  }

  function onkeydown(e: KeyboardEvent) {
    if (dialog) return;
    if (e.key === "Delete") void startDelete();
    else if (e.key === "F2" && chosen.length === 1) dialog = { kind: "rename", entry: chosen[0], value: chosen[0].name };
    else if (e.key === "Enter" && chosen.length === 1) open(chosen[0]);
    else if (e.key === "Backspace") go(parentPath(path, sep));
    else return;
    e.preventDefault();
  }
</script>

<section class="browser" class:highlight data-side={side}>
  <header>
    <span class="title">{title}</span>
    <span class="tools">
      <button class="icon-btn small" title="Up one folder" disabled={!files} onclick={() => go(parentPath(path, sep))}>
        <Icon name="up" size={14} />
      </button>
      <button class="icon-btn small" title="Home folder" disabled={!files} onclick={() => files && go(files.home)}>
        <Icon name="home" size={14} />
      </button>
      <button class="icon-btn small" title="Top folder (root)" disabled={!files} onclick={() => files && go(rootPath(path, sep))}>
        <Icon name="server" size={14} />
      </button>
      <button class="icon-btn small" title="Refresh" disabled={!files} onclick={reload}>
        <Icon name="refresh" size={14} />
      </button>
      <button class="icon-btn small" title="New folder" disabled={!files} onclick={() => (dialog = { kind: "mkdir", value: "" })}>
        <Icon name="folder-plus" size={14} />
      </button>
      <button
        class="icon-btn small"
        title="Rename (F2)"
        disabled={chosen.length !== 1}
        onclick={() => (dialog = { kind: "rename", entry: chosen[0], value: chosen[0].name })}
      >
        <Icon name="edit" size={14} />
      </button>
      {#if files?.chmod}
        <button
          class="icon-btn small"
          title="Permissions"
          disabled={chosen.length !== 1}
          onclick={() =>
            (dialog = {
              kind: "chmod",
              entry: chosen[0],
              value: chosen[0].permissions === null ? "" : (chosen[0].permissions & 0o777).toString(8),
            })}
        >
          <Icon name="key" size={14} />
        </button>
      {/if}
      <button class="icon-btn small" title="Delete (Del)" disabled={chosen.length === 0 || working} onclick={startDelete}>
        <Icon name="trash" size={14} />
      </button>
    </span>
  </header>

  <form
    class="pathbar"
    onsubmit={(e) => {
      e.preventDefault();
      go(typed.trim());
    }}
  >
    <input type="text" bind:value={typed} disabled={!files} spellcheck="false" aria-label="{title} folder" />
  </form>

  {#if status && !files}
    <div class="state">
      {#if status.endsWith("…")}<span class="spinner"></span>{/if}
      {status}
    </div>
  {:else if !files}
    <div class="state muted">Nothing open.</div>
  {:else}
    {#if error}<p class="message error inline">{error}</p>{/if}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="list" role="listbox" tabindex="0" aria-label="{title} files" aria-multiselectable="true" {onkeydown}>
      {#if parentPath(path, sep) !== path}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="row up" role="option" aria-selected="false" tabindex="-1" onclick={() => go(parentPath(path, sep))}>
          <span class="icon dir"><Icon name="up" size={14} /></span>
          <span class="name">..</span>
        </div>
      {/if}
      {#each visible as entry (entry.name)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row"
          class:selected={selected.includes(entry.name)}
          role="option"
          aria-selected={selected.includes(entry.name)}
          tabindex="-1"
          onpointerdown={(e) => rowPointerDown(entry, e)}
          onclick={(e) => pick(entry, e)}
          ondblclick={() => open(entry)}
        >
          <span class="icon" class:dir={entry.kind === "dir"}><Icon name={icon(entry)} size={14} /></span>
          <span class="name" title={entry.name}>{entry.name}</span>
          <span class="meta size">{formatSize(entry.size)}</span>
          <span class="meta when">{formatTime(entry.modified)}</span>
          {#if files.chmod && entry.permissions !== null}
            <span class="meta perm">{modeToText(entry.permissions & 0o777)}</span>
          {/if}
        </div>
      {/each}
      {#if !loading && visible.length === 0 && !error}
        <div class="state muted">This folder is empty.</div>
      {/if}
      {#if loading && entries.length === 0}
        <div class="state"><span class="spinner"></span>Loading…</div>
      {/if}
    </div>

    <footer>
      <label class="check small-check">
        <input type="checkbox" bind:checked={showHidden} /> Hidden files
      </label>
      <span class="count">
        {#if chosen.length}{chosen.length} selected{:else}{visible.length} items{/if}
      </span>
      {#if ontransfer}
        <button class="btn small primary" disabled={chosen.length === 0} onclick={transfer}>
          <Icon name={transferIcon} size={13} />
          {transferLabel}
        </button>
      {/if}
    </footer>
  {/if}
</section>

{#if dialog}
  <div class="modal-backdrop">
    <div
      class="modal"
      role="dialog"
      tabindex="-1"
      aria-label="File action"
      onkeydown={(e) => {
        if (e.key === "Escape") dialog = null;
        e.stopPropagation();
      }}
    >
      {#if dialog.kind === "mkdir"}
        <h2>New folder</h2>
        <label class="field">
          Name
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" autofocus bind:value={dialog.value} onkeydown={(e) => e.key === "Enter" && confirm()} />
        </label>
      {:else if dialog.kind === "rename"}
        <h2>Rename</h2>
        <label class="field">
          New name for {dialog.entry.name}
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" autofocus bind:value={dialog.value} onkeydown={(e) => e.key === "Enter" && confirm()} />
        </label>
      {:else if dialog.kind === "chmod"}
        <h2>Permissions</h2>
        <p class="lead">{dialog.entry.name}</p>
        <label class="field">
          Octal mode
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" autofocus bind:value={dialog.value} placeholder="755" onkeydown={(e) => e.key === "Enter" && confirm()} />
          <small>
            {#if parseMode(dialog.value) !== null}{modeToText(parseMode(dialog.value)!)}{:else}Three digits, for example 644 or 755.{/if}
          </small>
        </label>
      {:else}
        <h2>Delete {dialog.entries.length === 1 ? baseName(dialog.entries[0].name) : `${dialog.entries.length} items`}?</h2>
        <p class="lead">
          {#if dialog.folders > 0}
            This removes {dialog.files} file{dialog.files === 1 ? "" : "s"} and {dialog.folders} folder{dialog.folders === 1 ? "" : "s"}, and can't be undone.
          {:else}
            This can't be undone.
          {/if}
        </p>
      {/if}
      {#if dialogError}<p class="message error">{dialogError}</p>{/if}
      <div class="modal-actions">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn" autofocus={dialog.kind === "delete"} onclick={() => (dialog = null)}>Cancel</button>
        <button class="btn {dialog.kind === 'delete' ? 'danger' : 'primary'}" disabled={working} onclick={confirm}>
          {dialog.kind === "delete" ? "Delete" : dialog.kind === "mkdir" ? "Create" : "OK"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .browser {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .browser.highlight {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 2px var(--accent);
    border-radius: var(--radius);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 8px 4px 12px;
  }
  .title {
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .tools {
    display: flex;
  }
  .tools :global(.icon-btn:disabled) {
    opacity: 0.35;
    pointer-events: none;
  }
  .pathbar {
    padding: 0 10px 6px;
  }
  .pathbar input {
    padding: 4px 8px;
    font: 12px var(--mono);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0 6px;
    outline: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 8px;
    border-radius: var(--radius-s);
    cursor: default;
    user-select: none;
  }
  .row.up .name {
    color: var(--text-muted);
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.selected {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .icon {
    display: grid;
    color: var(--text-faint);
  }
  .icon.dir {
    color: var(--warning);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    flex: none;
    color: var(--text-faint);
    font-size: 11px;
    text-align: right;
  }
  .size {
    width: 58px;
  }
  .when {
    width: 108px;
  }
  .perm {
    width: 70px;
    font-family: var(--mono);
  }
  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px 8px;
    border-top: 1px solid var(--border);
  }
  .small-check {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
  }
  .count {
    flex: 1;
    color: var(--text-faint);
    font-size: 12px;
  }
  .state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px 12px;
    color: var(--text);
  }
  .state.muted {
    color: var(--text-faint);
  }
  .inline {
    margin: 0 10px 6px;
  }
  .spinner {
    width: 12px;
    height: 12px;
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
</style>
