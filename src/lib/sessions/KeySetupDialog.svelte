<script lang="ts">
  import { onMount } from "svelte";
  import {
    keychainDeleteSecret,
    keyGenerate,
    keyList,
    keySetup,
    keySetupCancel,
    promptRespond,
    type ExistingKey,
    type Prompt,
    type PromptResponse,
  } from "../backend";
  import PromptDialog from "../terminal/PromptDialog.svelte";
  import type { Library } from "./library.svelte";
  import type { SshSession } from "./tree";

  interface Props {
    library: Library;
    session: SshSession;
    onclose: () => void;
  }

  let { library, session, onclose }: Props = $props();

  type Stage = "choose" | "running" | "done" | "failed";
  let stage = $state<Stage>("choose");
  let existing = $state<ExistingKey[]>([]);
  let pick = $state("new"); // "new" or a private key path
  let passphrase = $state("");
  let step = $state("");
  let error = $state("");
  let question = $state<{ id: string; prompt: Prompt } | null>(null);
  let operationId = "";

  let usedKey = $state<{ path: string; line: string } | null>(null);
  let result = $state<{ already: boolean } | null>(null);
  let switchToKey = $state(true);
  let deletePassword = $state(false);

  onMount(() => {
    keyList().then(
      (keys) => (existing = keys),
      () => {},
    );
  });

  async function run() {
    stage = "running";
    error = "";
    operationId = crypto.randomUUID();
    try {
      let path: string;
      let line: string;
      if (pick === "new") {
        step = "Making a new key…";
        const made = await keyGenerate(passphrase || undefined);
        path = made.private_path;
        line = made.public_line;
      } else {
        const chosen = existing.find((k) => k.private_path === pick);
        if (!chosen) throw { message: "That key wasn't found." };
        path = chosen.private_path;
        line = chosen.public_line;
      }
      usedKey = { path, line };
      step = "Connecting and installing the key…";
      const outcome = await keySetup(operationId, session.id, path, line, (status) => {
        if (status.type === "prompt") question = { id: status.prompt_id, prompt: status.prompt };
      });
      result = { already: outcome.outcome === "already_present" };
      stage = "done";
    } catch (err) {
      error = (err as { message?: string }).message ?? "The key couldn't be set up.";
      stage = "failed";
    } finally {
      question = null;
    }
  }

  function answer(response: PromptResponse) {
    if (!question) return;
    void promptRespond(question.id, response).catch(() => {});
    question = null;
  }

  async function cancel() {
    if (stage === "running") await keySetupCancel(operationId).catch(() => {});
    onclose();
  }

  async function finish() {
    try {
      if (usedKey && switchToKey) {
        await library.save({
          ...library.file,
          session: library.file.session.map((s) =>
            s.id === session.id && s.type === "ssh" ? { ...s, auth: "key", key_path: usedKey!.path } : s,
          ),
        });
      }
      // Only if the user ticked it: a saved password is never removed unasked.
      if (deletePassword) await keychainDeleteSecret(session.id, "password").catch(() => {});
      onclose();
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't update the session.";
    }
  }
</script>

<div class="backdrop">
  <div
    class="dialog"
    role="dialog"
    aria-label="Set up key login"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape" && !question) void cancel();
      e.stopPropagation();
    }}
  >
    <h2>Key login for {session.name}</h2>

    {#if stage === "choose"}
      <p>
        This logs in to {session.user}@{session.host} once with your password, adds a key to the
        server, and checks that the key works before saying it's done.
      </p>
      <label class="choice">
        <input type="radio" bind:group={pick} value="new" /> Make a new key
      </label>
      {#if pick === "new"}
        <label class="indent">
          Passphrase (optional, but recommended)
          <input type="password" bind:value={passphrase} autocomplete="new-password" />
        </label>
      {/if}
      {#each existing as key (key.private_path)}
        <label class="choice">
          <input type="radio" bind:group={pick} value={key.private_path} />
          <span class="path">{key.private_path}</span>
        </label>
      {/each}
      <div class="buttons">
        <button onclick={cancel}>Cancel</button>
        <button class="primary" onclick={run}>Set up</button>
      </div>
    {:else if stage === "running"}
      <p>{step}</p>
      <div class="buttons"><button onclick={cancel}>Cancel</button></div>
    {:else if stage === "done"}
      <p class="ok">
        {result?.already ? "That key was already on the server." : "The key was added to the server."}
        Logging in with it worked.
      </p>
      <label class="choice">
        <input type="checkbox" bind:checked={switchToKey} /> Use this key for {session.name} from now on
      </label>
      {#if library.keychain && session.auth === "password"}
        <label class="choice">
          <input type="checkbox" bind:checked={deletePassword} /> Delete the saved password
        </label>
      {/if}
      {#if error}<p class="error">{error}</p>{/if}
      <div class="buttons"><button class="primary" onclick={finish}>Finish</button></div>
    {:else}
      <p class="error">{error}</p>
      {#if usedKey}<p class="small">The key file is {usedKey.path}</p>{/if}
      <div class="buttons">
        <button onclick={onclose}>Close</button>
        <button class="primary" onclick={() => (stage = "choose")}>Try again</button>
      </div>
    {/if}

    {#if question}
      <PromptDialog prompt={question.prompt} onanswer={answer} />
    {/if}
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
    position: relative;
    width: min(480px, 92vw);
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
    margin: 0 0 10px;
    font-size: 16px;
  }
  p {
    margin: 0 0 10px;
  }
  .choice {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 8px;
  }
  .path {
    word-break: break-all;
    font: 12px Consolas, monospace;
  }
  .indent {
    display: block;
    margin: 0 0 10px 24px;
  }
  input[type="password"] {
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
  .ok {
    color: #89d185;
  }
  .error {
    color: #f48771;
  }
  .small {
    color: #9d9d9d;
    font-size: 12px;
    word-break: break-all;
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
