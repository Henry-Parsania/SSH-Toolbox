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
    jump: editing?.jump ?? "",
  }));
  let name = $state(start.name);
  let host = $state(start.host);
  let port = $state<string | number>(start.port);
  let user = $state(start.user);
  let auth = $state<"password" | "key" | "agent">(start.auth);
  let keyPath = $state(start.keyPath);
  let inFolder = $state(start.folder);
  let jump = $state(start.jump);
  // Typed here only to be passed to the keychain. They never go in the sessions file.
  let password = $state("");
  let passphrase = $state("");

  let error = $state("");
  let saving = $state(false);

  const choices = $derived(folderChoices(library.file.folder));
  // Any other SSH session can be a jump host, unless it would go round in a circle.
  const jumpChoices = $derived(
    library.file.session.filter(
      (s): s is SshSession => s.type === "ssh" && s.id !== editing?.id && !goesThrough(s, editing?.id),
    ),
  );

  /** True if `s` already reaches the session being edited through its own jump hosts. */
  function goesThrough(s: SshSession, id: string | undefined): boolean {
    let at: SshSession | undefined = s;
    for (let i = 0; at && id && i < 10; i++) {
      if (at.jump === id) return true;
      at = library.file.session.find((x): x is SshSession => x.type === "ssh" && x.id === at!.jump);
    }
    return false;
  }

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
      jump,
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

<div class="modal-backdrop">
  <div
    class="modal"
    role="dialog"
    aria-label="Session"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>{editing ? "Edit session" : "New SSH session"}</h2>

    <label class="field">Name <input type="text" bind:value={name} /></label>
    <div class="row">
      <label class="field grow">Host <input type="text" bind:value={host} /></label>
      <label class="field port">Port <input type="number" min="1" max="65535" bind:value={port} /></label>
    </div>
    <label class="field">User <input type="text" bind:value={user} autocomplete="off" /></label>
    <label class="field">
      Folder
      <select bind:value={inFolder}>
        <option value="">(none)</option>
        {#each choices as choice (choice.id)}
          <option value={choice.id}>{choice.label}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      Login
      <select bind:value={auth}>
        <option value="password">Password</option>
        <option value="key">Private key file</option>
        <option value="agent">SSH agent</option>
      </select>
    </label>
    {#if auth === "key"}
      <label class="field">
        Private key file
        <input type="text" bind:value={keyPath} placeholder="~/.ssh/id_ed25519" />
        <small>The app reads this file when connecting and never copies it.</small>
      </label>
    {/if}
    <label class="field">
      Go through a jump host
      <select bind:value={jump}>
        <option value="">(connect directly)</option>
        {#each jumpChoices as j (j.id)}
          <option value={j.id}>{j.name} ({j.user}@{j.host})</option>
        {/each}
      </select>
      <small>Connects to that session first, then on to this one. Up to 3 hops.</small>
    </label>
    {#if library.keychain && auth === "password"}
      <label class="field">
        Save password in the keychain (optional)
        <input type="password" bind:value={password} autocomplete="new-password" />
        <small>Leave empty to be asked each time.</small>
      </label>
    {/if}
    {#if library.keychain && auth === "key"}
      <label class="field">
        Save the key's passphrase in the keychain (optional)
        <input type="password" bind:value={passphrase} autocomplete="new-password" />
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
