<script lang="ts">
  // Compares the folder shown on this computer with the one shown on the server and copies what
  // is missing or newer, in whichever direction is chosen. Nothing is ever deleted.

  import { onMount } from "svelte";
  import { filesList, filesMkdir, transferAdd, type OpenedFiles } from "../backend/files";
  import { compareFolders, relativeTo, type Comparison, type Item } from "./compare";
  import { joinPath, parentPath } from "./paths";

  interface Props {
    local: OpenedFiles;
    remote: OpenedFiles;
    localPath: string;
    remotePath: string;
    onclose: () => void;
  }

  let { local, remote, localPath, remotePath, onclose }: Props = $props();

  const MAX_FILES = 5000;
  const MAX_DEPTH = 12;

  let scanning = $state(true);
  let progress = $state("");
  let error = $state("");
  let note = $state("");
  let result = $state<Comparison | null>(null);
  let copying = $state(false);
  let cancelled = false;

  /** Every file under `root`, by its path relative to `root`. Links aren't followed. */
  async function scan(side: OpenedFiles, root: string): Promise<Map<string, Item>> {
    const found = new Map<string, Item>();
    const queue: { dir: string; depth: number }[] = [{ dir: root, depth: 0 }];
    while (queue.length && !cancelled) {
      const { dir, depth } = queue.shift()!;
      progress = `Reading ${dir}`;
      let entries;
      try {
        entries = await filesList(side.id, dir);
      } catch {
        continue; // A folder that can't be read is left out.
      }
      for (const e of entries) {
        if (e.kind === "dir" && depth < MAX_DEPTH) queue.push({ dir: joinPath(dir, e.name, side.separator), depth: depth + 1 });
        else if (e.kind === "file") {
          if (found.size >= MAX_FILES) throw { message: `There are more than ${MAX_FILES} files. Compare a smaller folder.` };
          found.set(relativeTo(root, joinPath(dir, e.name, side.separator)), { size: e.size, modified: e.modified });
        }
      }
    }
    return found;
  }

  onMount(() => {
    (async () => {
      try {
        const [l, r] = [await scan(local, localPath), await scan(remote, remotePath)];
        if (!cancelled) result = compareFolders(l, r);
      } catch (e) {
        error = (e as { message?: string }).message ?? "Couldn't compare the folders.";
      } finally {
        scanning = false;
      }
    })();
    return () => (cancelled = true);
  });

  const toServer = $derived(result ? [...result.onlyLocal, ...result.localNewer] : []);
  const toHere = $derived(result ? [...result.onlyRemote, ...result.remoteNewer] : []);

  /** Copies the listed files (relative paths) from one side to the other, making folders first. */
  async function copy(paths: string[], from: OpenedFiles, fromRoot: string, to: OpenedFiles, toRoot: string) {
    copying = true;
    error = "";
    note = "";
    try {
      const made = new Set<string>();
      for (const rel of paths) {
        const target = joinPath(toRoot, rel.split("/").join(to.separator), to.separator);
        const dir = parentPath(target, to.separator);
        // Folders that are missing are made, from the top down.
        const missing: string[] = [];
        for (let d = dir; d.length > toRoot.length && !made.has(d); d = parentPath(d, to.separator)) missing.unshift(d);
        for (const d of missing) {
          await filesMkdir(to.id, d).catch(() => {});
          made.add(d);
        }
        const source = joinPath(fromRoot, rel.split("/").join(from.separator), from.separator);
        await transferAdd(from.id, source, to.id, dir);
      }
      note = `${paths.length} ${paths.length === 1 ? "file was" : "files were"} added to the transfer list. Answer its questions about replacing newer copies there.`;
    } catch (e) {
      error = (e as { message?: string }).message ?? "Couldn't start the copy.";
    } finally {
      copying = false;
    }
  }

  const preview = (list: string[]) => list.slice(0, 200);
</script>

<div class="modal-backdrop">
  <div
    class="modal wide"
    role="dialog"
    aria-label="Compare folders"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>Compare folders</h2>
    <p class="path"><strong>This computer:</strong> {localPath}</p>
    <p class="path"><strong>Server:</strong> {remotePath}</p>

    {#if scanning}
      <p>Comparing… <span class="small">{progress}</span></p>
    {:else if error && !result}
      <p class="message error">{error}</p>
    {:else if result}
      <p class="summary">
        {result.same} the same · {result.onlyLocal.length} only here · {result.onlyRemote.length} only on the server ·
        {result.localNewer.length} newer here · {result.remoteNewer.length} newer on the server
        {#if result.differs.length} · {result.differs.length} different (same time){/if}
      </p>
      <div class="lists">
        {#each [["Only on this computer", result.onlyLocal], ["Only on the server", result.onlyRemote], ["Newer on this computer", result.localNewer], ["Newer on the server", result.remoteNewer], ["Different size, same time", result.differs]] as [title, list] (title)}
          {#if (list as string[]).length}
            <details>
              <summary>{title} ({(list as string[]).length})</summary>
              <ul>
                {#each preview(list as string[]) as p (p)}<li>{p}</li>{/each}
                {#if (list as string[]).length > 200}<li class="small">… and {(list as string[]).length - 200} more</li>{/if}
              </ul>
            </details>
          {/if}
        {/each}
      </div>
      {#if result.onlyLocal.length + result.onlyRemote.length + result.localNewer.length + result.remoteNewer.length + result.differs.length === 0}
        <p class="message ok">The folders match.</p>
      {/if}
    {/if}
    {#if note}<p class="message ok">{note}</p>{/if}
    {#if error && result}<p class="message error">{error}</p>{/if}

    <div class="modal-actions">
      <button class="btn" onclick={onclose}>Close</button>
      <button class="btn" disabled={copying || toHere.length === 0} onclick={() => copy(toHere, remote, remotePath, local, localPath)}>
        Copy {toHere.length} to this computer
      </button>
      <button class="btn primary" disabled={copying || toServer.length === 0} onclick={() => copy(toServer, local, localPath, remote, remotePath)}>
        Copy {toServer.length} to the server
      </button>
    </div>
    <p class="small">Only missing and newer files are copied. Nothing is deleted on either side.</p>
  </div>
</div>

<style>
  .path {
    margin: 2px 0;
    font: 12px var(--mono);
    overflow-wrap: anywhere;
  }
  .summary {
    margin: 12px 0 8px;
  }
  .lists {
    max-height: 300px;
    overflow: auto;
  }
  summary {
    padding: 4px 0;
    cursor: pointer;
  }
  ul {
    margin: 0 0 6px;
    padding-left: 22px;
    font: 12px var(--mono);
  }
  .small {
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
