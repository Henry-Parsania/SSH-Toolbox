<script lang="ts">
  import { untrack } from "svelte";
  import { keychainSet } from "../backend";
  import type { Library } from "./library.svelte";
  import { checkFtpForm, folderChoices, newId, type FtpSession } from "./tree";

  interface Props {
    library: Library;
    /** The session being changed, or none for a new one. */
    editing?: FtpSession;
    /** The folder a new session starts in. */
    folder?: string;
    onclose: () => void;
  }

  let { library, editing, folder = "", onclose }: Props = $props();

  const start = untrack(() => ({
    name: editing?.name ?? "",
    host: editing?.host ?? "",
    port: editing?.port ?? 21,
    user: editing?.user ?? "",
    security: editing?.security ?? ("explicit" as const),
    folder: editing?.folder ?? folder,
  }));
  let name = $state(start.name);
  let host = $state(start.host);
  let port = $state<string | number>(start.port);
  let user = $state(start.user);
  let security = $state<"plain" | "explicit" | "implicit">(start.security);
  let inFolder = $state(start.folder);
  // Typed here only to be passed to the keychain. It never goes in the sessions file.
  let password = $state("");
  let error = $state("");
  let saving = $state(false);

  const choices = $derived(folderChoices(library.file.folder));

  // The usual port follows the kind of connection, unless a different one was typed.
  function chooseSecurity(next: "plain" | "explicit" | "implicit") {
    const usual = (s: string) => (s === "implicit" ? 990 : 21);
    if (Number(port) === usual(security)) port = usual(next);
    security = next;
  }

  async function save() {
    const problem = checkFtpForm({ name, host, port });
    if (problem) {
      error = problem;
      return;
    }
    saving = true;
    error = "";
    const id = editing?.id ?? newId();
    const same =
      editing &&
      editing.host === host.trim() &&
      editing.port === Number(port) &&
      editing.security === security;
    const session: FtpSession = {
      type: "ftp",
      id,
      name: name.trim(),
      folder: inFolder,
      host: host.trim(),
      port: Number(port),
      user: user.trim(),
      security,
      // A certificate that was accepted belongs to that server. A different one starts again.
      trusted_cert_sha256: same && security !== "plain" ? editing!.trusted_cert_sha256 : "",
    };
    const sessions = editing
      ? library.file.session.map((s) => (s.id === id ? session : s))
      : [...library.file.session, session];
    try {
      await library.save({ ...library.file, session: sessions });
      if (library.keychain && password && session.user) {
        await keychainSet(id, "ftp_password", password).catch(() => {});
      }
      password = "";
      onclose();
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't save the session.";
    } finally {
      saving = false;
    }
  }
</script>

<div class="modal-backdrop">
  <div
    class="modal"
    role="dialog"
    aria-label="FTP session"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>{editing ? "Edit FTP session" : "New FTP session"}</h2>

    <label class="field">Name <input type="text" bind:value={name} /></label>
    <label class="field">
      Connection
      <select value={security} onchange={(e) => chooseSecurity(e.currentTarget.value as typeof security)}>
        <option value="explicit">FTPS, explicit TLS (recommended)</option>
        <option value="implicit">FTPS, implicit TLS</option>
        <option value="plain">FTP, no encryption</option>
      </select>
    </label>
    {#if security === "plain"}
      <p class="message warn">
        Plain FTP sends your password and files without protection. You will be warned each time you
        connect.
      </p>
    {/if}
    <div class="row">
      <label class="field grow">Host <input type="text" bind:value={host} /></label>
      <label class="field port">Port <input type="number" min="1" max="65535" bind:value={port} /></label>
    </div>
    <label class="field">
      User
      <input type="text" bind:value={user} autocomplete="off" />
      <small>Leave empty to log in anonymously.</small>
    </label>
    <label class="field">
      Folder
      <select bind:value={inFolder}>
        <option value="">(none)</option>
        {#each choices as choice (choice.id)}
          <option value={choice.id}>{choice.label}</option>
        {/each}
      </select>
    </label>
    {#if library.keychain && user.trim()}
      <label class="field">
        Save password in the keychain (optional)
        <input type="password" bind:value={password} autocomplete="new-password" />
        <small>Leave empty to be asked each time.</small>
      </label>
    {/if}

    {#if error}<p class="message error">{error}</p>{/if}
    <div class="modal-actions">
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" disabled={saving} onclick={save}>Save</button>
    </div>
  </div>
</div>

<style>
  .port {
    flex: none;
    width: 7em;
  }
</style>
