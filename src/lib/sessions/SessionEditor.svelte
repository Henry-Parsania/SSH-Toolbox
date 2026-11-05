<script lang="ts">
  import { untrack } from "svelte";
  import { keychainSet } from "../backend";
  import type { Library } from "./library.svelte";
  import { COLOR_CHOICES } from "./colors.svelte";
  import { checkSshForm, folderChoices, newId, type SshSession } from "./tree";

  interface Props {
    library: Library;
    /** The session being changed, or none for a new one. */
    editing?: SshSession;
    /** The folder a new session starts in. */
    folder?: string;
    /** An address to start a new session with, such as one found by the network scanner. */
    prefill?: { host: string; port: number; user?: string; connect?: boolean };
    onclose: () => void;
    /** Opens the "set up key login" dialog for a saved session. */
    onsetupkey?: (session: SshSession) => void;
    /** Called with the saved session when the form was started by quick connect. */
    onsaved?: (session: SshSession) => void;
  }

  let { library, editing, folder = "", prefill, onclose, onsetupkey, onsaved }: Props = $props();

  // The form starts from the session being edited, and doesn't follow later changes to it.
  const start = untrack(() => ({
    name: editing?.name ?? prefill?.host ?? "",
    host: editing?.host ?? prefill?.host ?? "",
    port: editing?.port ?? prefill?.port ?? 22,
    user: editing?.user ?? prefill?.user ?? "",
    auth: editing?.auth ?? ("password" as const),
    keyPath: editing?.key_path ?? "",
    folder: editing?.folder ?? folder,
    jump: editing?.jump ?? "",
    x11: editing?.x11 ?? false,
    color: editing?.color ?? "",
    startup: editing?.startup ?? "",
  }));
  let name = $state(start.name);
  let host = $state(start.host);
  let port = $state<string | number>(start.port);
  let user = $state(start.user);
  let auth = $state<"password" | "key" | "agent">(start.auth);
  let keyPath = $state(start.keyPath);
  let inFolder = $state(start.folder);
  let jump = $state(start.jump);
  let x11 = $state(start.x11);
  let copyKey = $state(false);
  let color = $state(start.color);
  let startup = $state(start.startup);
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
      x11,
      color,
      startup,
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
      if (copyKey && auth === "password") onsetupkey?.(session);
      else if (prefill?.connect && !editing) onsaved?.(session);
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

    <section class="card">
      <h3>Connection</h3>
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

    </section>

    <section class="card">
      <h3>Login</h3>
    <div class="field">
      Login with
      <div class="seg" role="radiogroup" aria-label="Login with">
        <button type="button" role="radio" aria-checked={auth === "password"} class:picked={auth === "password"} onclick={() => (auth = "password")}>Password</button>
        <button type="button" role="radio" aria-checked={auth === "key"} class:picked={auth === "key"} onclick={() => (auth = "key")}>Key file</button>
        <button type="button" role="radio" aria-checked={auth === "agent"} class:picked={auth === "agent"} onclick={() => (auth = "agent")}>SSH agent</button>
      </div>
    </div>
    {#if auth === "key"}
      <label class="field">
        Private key file
        <input type="text" bind:value={keyPath} placeholder="~/.ssh/id_ed25519" />
        <small>The app reads this file when connecting and never copies it.</small>
      </label>
    {/if}
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
    {#if auth === "password" && onsetupkey}
      <div class="field">
        <label class="check">
          <input type="checkbox" bind:checked={copyKey} />
          Copy an SSH key to the server after saving (like ssh-copy-id)
        </label>
        <small>
          Logs in once with your password, adds a key to the server, and switches this session to
          key login. You can pick an existing key or make a new one.
        </small>
      </div>
    {/if}

    </section>

    <section class="card">
      <h3>Appearance</h3>
    <div class="field">
      Colour
      <div class="swatches">
        <button type="button" class="swatch none" class:picked={!color} title="Follow the theme" aria-label="No colour" onclick={() => (color = "")}></button>
        {#each COLOR_CHOICES as c (c)}
          <button type="button" class="swatch" class:picked={color === c} style:background={c} title={c} aria-label={c} onclick={() => (color = c)}></button>
        {/each}
        <input type="color" value={color || "#1f3f7a"} oninput={(e) => (color = e.currentTarget.value)} title="Pick any colour" aria-label="Custom colour" />
      </div>
      <small>Colours this session's tab and terminal background, instead of the theme's. Handy for telling servers apart.</small>
    </div>

    </section>

    <section class="card">
      <h3>Advanced</h3>
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
    <div class="field">
      <label class="check">
        <input type="checkbox" bind:checked={x11} />
        Show windows of programs on the server (X11)
      </label>
      <small>Needs an X server on this computer (Tools, X server) and X11 forwarding allowed on the server.</small>
    </div>
    <label class="field">
      Commands to run after connecting (optional)
      <textarea rows="3" bind:value={startup} spellcheck="false" placeholder="cd /srv/app"></textarea>
      <small>Typed once, one line at a time, when the shell is ready. Don't put passwords here: this is saved as plain text.</small>
    </label>

    </section>

    {#if error}<p class="message error">{error}</p>{/if}
    <div class="modal-actions">
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" disabled={saving} onclick={save}>Save</button>
    </div>
  </div>
</div>

<style>
  textarea {
    font: 13px var(--mono);
    resize: vertical;
  }
  .swatches {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .swatch {
    width: 26px;
    height: 26px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 6px;
    cursor: pointer;
  }
  .swatch.none {
    background: var(--bg-0);
    border-color: var(--border);
  }
  .swatch.picked {
    border-color: var(--accent);
  }
  .swatches input[type="color"] {
    width: 34px;
    height: 28px;
    padding: 0;
    background: none;
    border: 0;
  }
  .port {
    flex: none;
    width: 7em;
  }
</style>
