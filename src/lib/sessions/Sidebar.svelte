<script lang="ts">
  import { untrack } from "svelte";
  import type { SavedSession } from "../backend";
  import { settings } from "../settings.svelte";
  import type { Reachability } from "../status/reach.svelte";
  import { fitMenu } from "../ui/fitMenu";
  import Icon from "../ui/Icon.svelte";
  import ForwardsDialog from "./ForwardsDialog.svelte";
  import FtpEditor from "./FtpEditor.svelte";
  import SerialEditor from "./SerialEditor.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import KeySetupDialog from "./KeySetupDialog.svelte";
  import type { Library } from "./library.svelte";
  import SessionEditor from "./SessionEditor.svelte";
  import { buildTree, newId, type FtpSession, type SerialSession, type SshSession, type TreeItem } from "./tree";

  interface Props {
    library: Library;
    /** Open a saved session in a new tab. */
    onopen: (session: SavedSession) => void;
    /** Open the server's process monitor (btop, htop or top) in a new tab. */
    onmonitor?: (session: SavedSession) => void;
    /** Each time this goes up, the editor for a new session opens. */
    newSessionSignal?: number;
    /** A request from the menu bar. Each one has its own number, so the same command can repeat. */
    command?: { name: "ssh" | "ftp" | "serial" | "import"; n: number; prefill?: { host: string; port: number; user?: string; connect?: boolean } } | null;
    /** Which servers answer, shown as a dot beside each one. */
    reach?: Reachability;
    /** The saved session of the active tab, highlighted in the list. */
    activeId?: string | null;
  }

  let { library, onopen, onmonitor, newSessionSignal = 0, command = null, reach, activeId = null }: Props = $props();

  let search = $state("");
  let collapsed = $state<Record<string, boolean>>({});
  let editor = $state<{ editing?: SshSession; folder?: string; prefill?: { host: string; port: number; user?: string; connect?: boolean } } | null>(null);
  let confirm = $state<{ kind: "session" | "folder"; id: string; name: string } | null>(null);
  let ftpEditor = $state<{ editing?: FtpSession; folder?: string } | null>(null);
  let serialEditor = $state<{ editing?: SerialSession; folder?: string } | null>(null);
  let importing = $state(false);
  let forwardsFor = $state<SshSession | null>(null);
  let keySetupFor = $state<SshSession | null>(null);
  let renaming = $state<{ id: string; name: string } | null>(null);
  let message = $state("");
  // The right-click menu: the same actions as the small buttons on the row.
  let menu = $state<{ x: number; y: number; target: { folder: TreeItem & { kind: "folder" } } | { session: SavedSession } | { add: true } } | null>(null);

  function openMenu(e: MouseEvent, target: NonNullable<typeof menu>["target"]) {
    e.preventDefault();
    e.stopPropagation();
    menu = { x: e.clientX, y: e.clientY, target };
  }

  /** Opens the same menu from a small "more" button, just under it. */
  function openMenuFrom(e: MouseEvent, target: NonNullable<typeof menu>["target"]) {
    e.stopPropagation();
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { x: box.left, y: box.bottom + 2, target };
  }

  /** Runs a menu action, then closes the menu. The action goes first because it reads the menu's target. */
  function act(action: () => void) {
    action();
    menu = null;
  }

  const tree = $derived(buildTree(library.file, search));

  $effect(() => {
    if (newSessionSignal > 0) editor = {};
  });

  $effect(() => {
    if (!command) return;
    const name = command.name;
    untrack(() => {
      if (name === "ssh") editor = { prefill: command?.prefill };
      else if (name === "ftp") ftpEditor = {};
      else if (name === "serial") serialEditor = {};
      else importing = true;
    });
  });

  /** Saves a copy of a session next to it. Saved passwords stay with the original: the copy asks. */
  async function duplicate(session: SavedSession) {
    const copy = { ...session, id: newId(), name: `${session.name} (copy)` };
    const at = library.file.session.findIndex((s) => s.id === session.id);
    const sessions = [...library.file.session];
    sessions.splice(at + 1, 0, copy);
    try {
      await library.save({ ...library.file, session: sessions });
    } catch {
      /* The sessions list shows its own error if saving fails. */
    }
  }

  function describe(session: SavedSession): string {
    if (session.type === "ssh") return `${session.user}@${session.host}`;
    if (session.type === "serial") return session.port;
    return session.host;
  }

  /** The dot beside a session: how it answered the last check, if it is checked at all. */
  function dot(session: SavedSession): { state: "up" | "down" | "unknown"; tip: string } | null {
    if (!reach || !settings.reachability_enabled) return null;
    if (session.type === "serial" || (session.type === "ssh" && session.jump)) return null;
    const answer = reach.results[session.id];
    if (!answer) return { state: "unknown", tip: reach.checking ? "Checking..." : "Not checked yet" };
    return answer.reachable
      ? { state: "up", tip: `Reachable${answer.millis === null ? "" : ` (${answer.millis} ms)`}` }
      : { state: "down", tip: "Did not answer the last check" };
  }

  function iconOf(session: SavedSession) {
    return session.type === "serial" ? "serial" : session.type === "ftp" ? "ftp" : "server";
  }

  async function run(action: () => Promise<void>) {
    message = "";
    try {
      await action();
    } catch (err) {
      message = (err as { message?: string }).message ?? "That didn't work.";
    }
  }

  function addFolder(parent: string) {
    renaming = { id: `new:${parent}`, name: "" };
  }

  async function finishRename(save: boolean) {
    const target = renaming;
    renaming = null;
    if (!target || !save || !target.name.trim()) return;
    const name = target.name.trim();
    await run(() => {
      if (target.id.startsWith("new:")) {
        const folder = { id: newId(), name, parent: target.id.slice(4) };
        return library.save({ ...library.file, folder: [...library.file.folder, folder] });
      }
      return library.save({
        ...library.file,
        folder: library.file.folder.map((f) => (f.id === target.id ? { ...f, name } : f)),
      });
    });
  }

  async function doDelete() {
    const target = confirm;
    confirm = null;
    if (!target) return;
    await run(() =>
      target.kind === "session" ? library.deleteSession(target.id) : library.deleteFolder(target.id),
    );
  }
</script>

{#snippet renameBox(depth: number)}
  <div class="item" style:padding-left="{10 + depth * 14}px">
    <span class="glyph folder-glyph"><Icon name="folder" size={15} /></span>
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="rename"
      type="text"
      autofocus
      placeholder="Folder name"
      bind:value={renaming!.name}
      onblur={() => finishRename(true)}
      onkeydown={(e) => {
        if (e.key === "Enter") finishRename(true);
        else if (e.key === "Escape") finishRename(false);
        e.stopPropagation();
      }}
    />
  </div>
{/snippet}

{#snippet row(item: TreeItem, depth: number)}
  {#if item.kind === "folder"}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="item folder" style:padding-left="{6 + depth * 14}px" oncontextmenu={(e) => openMenu(e, { folder: item })}>
      <button
        class="label"
        onclick={() => (collapsed[item.folder.id] = !collapsed[item.folder.id])}
        ondblclick={() => (renaming = { id: item.folder.id, name: item.folder.name })}
      >
        <span class="chev" class:open={!collapsed[item.folder.id] || !!search}>
          <Icon name="chevron-right" size={13} />
        </span>
        <span class="glyph folder-glyph"><Icon name="folder" size={15} /></span>
        <span class="name">{item.folder.name}</span>
        <span class="count">{item.children.length}</span>
      </button>
      <span class="tools">
        <button class="icon-btn small" title="New session here" onclick={() => (editor = { folder: item.folder.id })}>
          <Icon name="plus" size={13} />
        </button>
        <button class="icon-btn small" title="More: new folder, rename, delete" aria-label="More" onclick={(e) => openMenuFrom(e, { folder: item })}>
          <Icon name="more" size={13} />
        </button>
      </span>
    </div>
    {#if renaming?.id === item.folder.id}
      {@render renameBox(depth + 1)}
    {/if}
    {#if !collapsed[item.folder.id] || search}
      {#each item.children as child (child.kind === "folder" ? child.folder.id : child.session.id)}
        {@render row(child, depth + 1)}
      {/each}
    {/if}
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="item" class:current={item.session.id === activeId} style:padding-left="{6 + depth * 14 + 13}px" oncontextmenu={(e) => openMenu(e, { session: item.session })}>
      <button class="label" title="{item.session.name}: {describe(item.session)}" ondblclick={() => onopen(item.session)}>
        <span class="glyph"><Icon name={iconOf(item.session)} size={15} /></span>
        {#if dot(item.session)}
          {@const d = dot(item.session)!}
          <span class="dot {d.state}" title={d.tip} aria-label={d.tip}></span>
        {/if}
        <span class="name">{item.session.name}</span>
        <span class="where">{describe(item.session)}</span>
      </button>
      <span class="tools">
        <button class="icon-btn small accent" title="Connect in a new tab" onclick={() => onopen(item.session)}>
          <Icon name="play" size={13} />
        </button>
        <button class="icon-btn small" title="More: edit, monitor, forwarding, key login, duplicate, delete" aria-label="More" onclick={(e) => openMenuFrom(e, { session: item.session })}>
          <Icon name="more" size={13} />
        </button>
      </span>
    </div>
  {/if}
{/snippet}

<svelte:window onpointerdown={() => (menu = null)} onblur={() => (menu = null)} onkeydown={(e) => e.key === "Escape" && (menu = null)} />

{#if menu}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <ul class="menu ctx" role="menu" use:fitMenu={{ x: menu.x, y: menu.y }} onpointerdown={(e) => e.stopPropagation()} oncontextmenu={(e) => e.preventDefault()}>
    {#if "add" in menu.target}
      <li><button onclick={() => act(() => (editor = {}))}><Icon name="server" size={14} /> New SSH session</button></li>
      <li><button onclick={() => act(() => (ftpEditor = {}))}><Icon name="ftp" size={14} /> New FTP session</button></li>
      <li><button onclick={() => act(() => (serialEditor = {}))}><Icon name="serial" size={14} /> New serial session</button></li>
      <li class="sep" role="separator"></li>
      <li><button onclick={() => act(() => addFolder(""))}><Icon name="folder-plus" size={14} /> New folder</button></li>
      <li><button onclick={() => act(() => (importing = true))}><Icon name="import" size={14} /> Import from ~/.ssh/config</button></li>
    {:else if "folder" in menu.target}
      {@const f = menu.target.folder.folder}
      <li><button onclick={() => act(() => (editor = { folder: f.id }))}><Icon name="plus" size={14} /> New session here</button></li>
      <li><button onclick={() => act(() => addFolder(f.id))}><Icon name="folder-plus" size={14} /> New folder inside</button></li>
      <li><button onclick={() => act(() => (renaming = { id: f.id, name: f.name }))}><Icon name="edit" size={14} /> Rename</button></li>
      <li class="sep" role="separator"></li>
      <li><button onclick={() => act(() => (confirm = { kind: "folder", id: f.id, name: f.name }))}><Icon name="trash" size={14} /> Delete folder</button></li>
    {:else}
      {@const sess = menu.target.session}
      <li><button onclick={() => act(() => onopen(sess))}><Icon name="play" size={14} /> Connect in a new tab</button></li>
      {#if sess.type === "ssh"}
        <li><button onclick={() => act(() => onmonitor?.(sess))}><Icon name="monitor" size={14} /> Process monitor</button></li>
        <li><button onclick={() => act(() => (forwardsFor = sess))}><Icon name="route" size={14} /> Port forwarding</button></li>
        <li><button onclick={() => act(() => (keySetupFor = sess))}><Icon name="key" size={14} /> Set up key login</button></li>
      {/if}
      <li class="sep" role="separator"></li>
      <li>
        <button
          onclick={() =>
            act(() => {
              if (sess.type === "ssh") editor = { editing: sess };
              else if (sess.type === "ftp") ftpEditor = { editing: sess };
              else serialEditor = { editing: sess };
            })}><Icon name="edit" size={14} /> Edit</button
        >
      </li>
      <li><button onclick={() => act(() => void duplicate(sess))}><Icon name="copy" size={14} /> Duplicate</button></li>
      <li><button onclick={() => act(() => (confirm = { kind: "session", id: sess.id, name: sess.name }))}><Icon name="trash" size={14} /> Delete</button></li>
    {/if}
  </ul>
{/if}

<aside class="sidebar">
  <header>
    <span class="heading">Sessions</span>
    <span class="head-tools">
      <button class="icon-btn" title="New SSH session" aria-label="New SSH session" onclick={() => (editor = {})}>
        <Icon name="plus" size={16} />
      </button>
      <button class="icon-btn" title="More: new FTP or serial session, new folder, import" aria-label="More" onclick={(e) => openMenuFrom(e, { add: true })}>
        <Icon name="more" size={16} />
      </button>
    </span>
  </header>

  <div class="search-wrap">
    <span class="search-icon"><Icon name="search" size={14} /></span>
    <input class="search" type="search" placeholder="Search sessions" bind:value={search} />
  </div>

  {#if library.problem}
    <div class="problem">
      <strong>The saved sessions can't be read</strong>
      <p>{library.problem}</p>
      <p>
        The file hasn't been changed. Fix it by hand, or start again (the old file is kept as
        sessions.toml.bad).
      </p>
      <button class="btn small danger" onclick={() => run(() => library.startAgain())}>Start again</button>
    </div>
  {/if}

  {#if message}<p class="message error side-message">{message}</p>{/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="list" oncontextmenu={(e) => openMenu(e, { add: true })}>
    {#if renaming?.id === "new:"}
      {@render renameBox(0)}
    {/if}
    {#each tree as item (item.kind === "folder" ? item.folder.id : item.session.id)}
      {@render row(item, 0)}
    {/each}

    {#if tree.length === 0 && !library.problem}
      <div class="empty">
        {#if search}
          <p>Nothing matches "{search}".</p>
        {:else}
          <div class="empty-icon"><Icon name="server" size={24} /></div>
          <p><strong>No saved sessions yet</strong></p>
          <p class="hint">Save the servers you use, then open them with a double-click.</p>
          <div class="empty-actions">
            <button class="btn small primary" onclick={() => (editor = {})}>
              <Icon name="plus" size={13} /> New session
            </button>
            <button class="btn small" onclick={() => (importing = true)}>
              <Icon name="import" size={13} /> Import
            </button>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</aside>

{#if editor}
  <SessionEditor {library} editing={editor.editing} folder={editor.folder} prefill={editor.prefill} onclose={() => (editor = null)} onsetupkey={(s) => (keySetupFor = s)} onsaved={onopen} />
{/if}

{#if serialEditor}
  <SerialEditor {library} editing={serialEditor.editing} folder={serialEditor.folder} onclose={() => (serialEditor = null)} />
{/if}

{#if ftpEditor}
  <FtpEditor {library} editing={ftpEditor.editing} folder={ftpEditor.folder} onclose={() => (ftpEditor = null)} />
{/if}

{#if forwardsFor}
  <ForwardsDialog session={forwardsFor} onclose={() => (forwardsFor = null)} />
{/if}

{#if importing}
  <ImportDialog {library} onclose={() => (importing = false)} />
{/if}

{#if keySetupFor}
  <KeySetupDialog {library} session={keySetupFor} onclose={() => (keySetupFor = null)} />
{/if}

{#if confirm}
  <div class="modal-backdrop">
    <div class="modal" role="alertdialog" aria-label="Delete">
      <h2>Delete {confirm.kind === "folder" ? "folder" : "session"}?</h2>
      <p class="lead">
        <b>{confirm.name}</b>
        {#if confirm.kind === "folder"}will be removed. What's inside moves up one level.{:else}will be removed, along with its saved password.{/if}
      </p>
      <div class="modal-actions">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn" autofocus onclick={() => (confirm = null)}>Cancel</button>
        <button class="btn danger" onclick={doDelete}>Delete</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 100%;
    flex: none;
    min-height: 0;
    background: var(--bg-1);
    border-right: 1px solid var(--border-strong);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 8px 6px 14px;
  }
  .heading {
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0.02em;
  }
  .head-tools {
    display: flex;
    gap: 1px;
  }
  .search-wrap {
    position: relative;
    padding: 4px 10px 8px;
  }
  .search-icon {
    position: absolute;
    left: 20px;
    top: 50%;
    transform: translateY(-62%);
    color: var(--text-faint);
    pointer-events: none;
  }
  .search {
    padding-left: 30px;
    border-radius: var(--radius);
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 2px 6px 12px;
  }
  .item {
    display: flex;
    align-items: center;
    min-height: 30px;
    padding-right: 4px;
    border-radius: var(--radius-s);
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 4px;
    font: inherit;
    color: var(--text);
    text-align: left;
    background: none;
    border: none;
    cursor: default;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    margin-right: -2px;
    border-radius: 50%;
    background: var(--text-faint);
    opacity: 0.55;
  }
  .dot.up {
    background: #3fb97a;
    opacity: 1;
    box-shadow: 0 0 0 3px #3fb97a26;
  }
  /* Not answering is routine (a laptop that is off), so it is a quiet hollow dot, not an alarm. */
  .dot.down {
    background: transparent;
    border: 1.5px solid var(--text-faint);
    opacity: 0.9;
  }
  .item.current {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .glyph {
    display: grid;
    color: var(--text-faint);
  }
  .item:hover .glyph {
    color: var(--accent);
  }
  .glyph.folder-glyph,
  .item:hover .glyph.folder-glyph {
    color: var(--warning);
  }
  .chev {
    display: grid;
    margin-right: -2px;
    color: var(--text-faint);
    transition: transform 0.12s var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .name {
    flex: none;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }
  .where {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-faint);
    font-size: 12px;
  }
  .count {
    margin-left: auto;
    padding: 0 6px;
    color: var(--text-faint);
    font-size: 11px;
  }
  .ctx {
    position: fixed;
    z-index: 60;
    min-width: 200px;
  }
  .ctx li {
    list-style: none;
  }
  .ctx .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
  .tools {
    display: none;
    flex: none;
  }
  .item:hover .tools,
  .item:focus-within .tools {
    display: flex;
  }
  .item:hover .count {
    display: none;
  }
  .icon-btn.accent:hover {
    color: var(--go);
  }
  .rename {
    padding: 3px 8px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    margin: 28px 10px 0;
    padding: 18px 12px;
    text-align: center;
    color: var(--text-muted);
  }
  .empty p {
    margin: 0 0 6px;
  }
  .empty .hint {
    margin-bottom: 14px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .empty-icon {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    margin-bottom: 10px;
    color: var(--accent);
    background: var(--accent-soft);
    border-radius: 14px;
  }
  .empty-actions {
    display: flex;
    gap: 6px;
  }
  .problem {
    margin: 4px 10px 8px;
    padding: 10px 12px;
    color: var(--text);
    background: var(--danger-soft);
    border: 1px solid #f2706b44;
    border-radius: var(--radius);
  }
  .problem p {
    margin: 4px 0 8px;
    color: var(--text-muted);
    font-size: 12px;
  }
  .side-message {
    margin: 0 10px 8px;
  }
</style>
