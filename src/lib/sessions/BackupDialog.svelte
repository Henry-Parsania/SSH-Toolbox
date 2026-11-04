<script lang="ts">
  // Back up the saved sessions to one file, and bring them back on another computer. The file
  // holds hosts, users and key file paths, but no passwords and no key contents.

  import { sessionsExport, sessionsImport, type MergeResult } from "../backend";
  import type { Library } from "./library.svelte";

  interface Props {
    library: Library;
    onclose: () => void;
  }

  let { library, onclose }: Props = $props();

  let busy = $state(false);
  let saved = $state("");
  let path = $state("");
  let result = $state<MergeResult | null>(null);
  let error = $state("");

  async function run<T>(action: () => Promise<T>): Promise<T | undefined> {
    busy = true;
    error = "";
    try {
      return await action();
    } catch (e) {
      error = (e as { message?: string }).message ?? "That didn't work.";
    } finally {
      busy = false;
    }
  }

  async function backup() {
    saved = (await run(sessionsExport)) ?? "";
    result = null;
  }

  async function restore() {
    const done = await run(() => sessionsImport(path));
    if (!done) return;
    result = done;
    saved = "";
    await library.load().catch(() => {});
  }
</script>

<div class="modal-backdrop">
  <div
    class="modal"
    role="dialog"
    aria-label="Back up and restore sessions"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>Back up and restore sessions</h2>

    <h3>Back up</h3>
    <p class="lead">
      Saves your sessions and folders to a file in your Downloads folder. It has no passwords and
      no private keys, only the names of key files. Passwords saved in the keychain are not
      included, so they are asked for again on the new computer.
    </p>
    <button class="btn primary" disabled={busy} onclick={backup}>Save a backup</button>
    {#if saved}<p class="message ok">Saved to {saved}</p>{/if}

    <h3>Restore</h3>
    <label class="field">
      Backup file
      <input type="text" bind:value={path} placeholder="Path of a sessions backup (.toml)" spellcheck="false" />
      <small>
        Sessions that aren't here yet are added. The ones you already have are left as they are.
        Trusted FTPS certificates are not restored: you are asked to accept them again.
      </small>
    </label>
    <button class="btn" disabled={busy || !path.trim()} onclick={restore}>Restore</button>
    {#if result}
      <p class="message ok">
        Added {result.sessions_added} {result.sessions_added === 1 ? "session" : "sessions"} and
        {result.folders_added} {result.folders_added === 1 ? "folder" : "folders"}.
        {#if result.skipped > 0}{result.skipped} already here, left as they are.{/if}
      </p>
    {/if}

    {#if error}<p class="message error">{error}</p>{/if}
    <div class="modal-actions"><button class="btn primary" onclick={onclose}>Close</button></div>
  </div>
</div>

<style>
  h3 {
    margin: 16px 0 6px;
    font-size: 13px;
  }
</style>
