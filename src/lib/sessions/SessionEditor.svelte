<script lang="ts">
  import { untrack } from "svelte";
  import { keychainSet } from "../backend";
  import type { Library } from "./library.svelte";
  import { checkSshForm, folderChoices, newId, type SshSession } from "./tree";

  interface Props {
    library: Library;
    /** The session being changed, or none for a new one. */
    editing?: SshSession;
    /** The folder a new session starts in. */
    folder?: string;
    onclose: () => void;
  }

  let { library, editing, folder = "", onclose }: Props = $props();

  // The form starts from the session being edited, and doesn't follow later changes to it.
  const start = untrack(() => ({
    name: editing?.name ?? "",
    host: editing?.host ?? "",
    port: editing?.port ?? 22,
    user: editing?.user ?? "",
    auth: editing?.auth ?? ("password" as const),
    keyPath: editing?.key_path ?? "",
    folder: editing?.folder ?? folder,
  }));
  let name = $state(start.name);
  let host = $state(start.host);
  let port = $state<string | number>(start.port);
  let user = $state(start.user);
  let auth = $state<"password" | "key" | "agent">(start.auth);
  let keyPath = $state(start.keyPath);
  let inFolder = $state(start.folder);
  // Typed here only to be passed to the keychain. They never go in the sessions file.
  let password = $state("");
  let passphrase = $state("");

  let error = $state("");
  let saving = $state(false);

  const choices = $derived(folderChoices(library.file.folder));

  async function save() {
    const problem = checkSshForm({ name, host, port, user, auth, key_path: keyPath });
    if (problem) {
      error = problem;
      return;
    }
    saving = true;
    error = "";
    const id = editing?.id ?? newId();
    const session: SshSession = {
      type: "ssh",
      id,
      name: name.trim(),
      folder: inFolder,
      host: host.trim(),
      port: Number(port),
      user: user.trim(),
      auth,
      key_path: auth === "key" ? keyPath.trim() : "",
      jump: editing?.jump ?? "",
    };
    const sessions = editing
      ? library.file.session.map((s) => (s.id === id ? session : s))
      : [...library.file.session, session];
    try {
      await library.save({ ...library.file, session: sessions });
      // Saved secrets go to the keychain, never the file. If that fails the session is still
      // saved, and the password will be asked for when connecting.
      if (library.keychain) {
        if (auth === "password" && password) await keychainSet(id, "password", password).catch(() => {});
        if (auth === "key" && passphrase) await keychainSet(id, "passphrase", passphrase).catch(() => {});
      }
      password = "";
      passphrase = "";
      onclose();
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't save the session.";
    } finally {
      saving = false;
    }
  }
</script>

<div class="backdrop">
  <div
    class="dialog"
    role="dialog"
    aria-label="Session"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>{editing ? "Edit session" : "New SSH session"}</h2>

    <label>Name <input type="text" bind:value={name} /></label>
    <div class="row">
      <label class="grow">Host <input type="text" bind:value={host} /></label>
      <label class="port">Port <input type="number" min="1" max="65535" bind:value={port} /></label>
    </div>
    <label>User <input type="text" bind:value={user} autocomplete="off" /></label>
    <label>
      Folder
      <select bind:value={inFolder}>
        <option value="">(none)</option>
        {#each choices as choice (choice.id)}
          <option value={choice.id}>{choice.label}</option>
        {/each}
      </select>
    </label>
    <label>
      Login
      <select bind:value={auth}>
        <option value="password">Password</option>
        <option value="key">Private key file</option>
        <option value="agent">SSH agent</option>
      </select>
    </label>
    {#if auth === "key"}
      <label>
        Private key file
        <input type="text" bind:value={keyPath} placeholder="~/.ssh/id_ed25519" />
        <small>The app reads this file when connecting and never copies it.</small>
      </label>
    {/if}
    {#if library.keychain && auth === "password"}
      <label>
        Save password in the keychain (optional)
        <input type="password" bind:value={password} autocomplete="new-password" />
        <small>Leave empty to be asked each time.</small>
      </label>
    {/if}
    {#if library.keychain && auth === "key"}
      <label>
        Save the key's passphrase in the keychain (optional)
        <input type="password" bind:value={passphrase} autocomplete="new-password" />
      </label>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}
    <div class="buttons">
      <button onclick={onclose}>Cancel</button>
      <button class="primary" disabled={saving} onclick={save}>Save</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #0008;
  }
  .dialog {
    width: min(440px, 92vw);
    max-height: 92vh;
    overflow: auto;
    padding: 16px 20px;
    background: #252526;
    color: #d4d4d4;
    border: 1px solid #555;
    border-radius: 6px;
    font: 13px system-ui, sans-serif;
  }
  h2 {
    margin: 0 0 12px;
    font-size: 16px;
  }
  label {
    display: block;
    margin-bottom: 10px;
  }
  .row {
    display: flex;
    gap: 10px;
  }
  .grow {
    flex: 1;
  }
  .port {
    width: 6em;
  }
  input,
  select {
    display: block;
    width: 100%;
    box-sizing: border-box;
    margin-top: 3px;
    padding: 4px 6px;
    font: inherit;
    color: inherit;
    background: #3c3c3c;
    border: 1px solid #555;
  }
  small {
    display: block;
    margin-top: 2px;
    color: #9d9d9d;
  }
  .error {
    color: #f48771;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
  button {
    padding: 5px 14px;
    font: inherit;
    color: inherit;
    background: #3c3c3c;
    border: 1px solid #555;
    border-radius: 3px;
    cursor: pointer;
  }
  button.primary {
    background: #0e639c;
    border-color: #0e639c;
  }
</style>
