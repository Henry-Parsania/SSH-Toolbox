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

<div class="modal-backdrop">
  <div
    class="modal"
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
      <p class="lead">
        This logs in to {session.user}@{session.host} once with your password, adds a key to the
        server, and checks that the key works before saying it's done.
      </p>
      <label class="check">
        <input type="radio" bind:group={pick} value="new" /> Make a new key
      </label>
      {#if pick === "new"}
        <label class="field indent">
          Passphrase (optional, but recommended)
          <input type="password" bind:value={passphrase} autocomplete="new-password" />
        </label>
      {/if}
      {#each existing as key (key.private_path)}
        <label class="check">
          <input type="radio" bind:group={pick} value={key.private_path} />
          <span class="path">{key.private_path}</span>
        </label>
      {/each}
      <div class="modal-actions">
        <button class="btn" onclick={cancel}>Cancel</button>
        <button class="btn primary" onclick={run}>Set up</button>
      </div>
    {:else if stage === "running"}
      <p>{step}</p>
      <div class="modal-actions"><button class="btn" onclick={cancel}>Cancel</button></div>
    {:else if stage === "done"}
      <p class="message ok">
        {result?.already ? "That key was already on the server." : "The key was added to the server."}
        Logging in with it worked.
      </p>
      <label class="check">
        <input type="checkbox" bind:checked={switchToKey} /> Use this key for {session.name} from now on
      </label>
      {#if library.keychain && session.auth === "password"}
        <label class="check">
          <input type="checkbox" bind:checked={deletePassword} /> Delete the saved password
        </label>
      {/if}
      {#if error}<p class="message error">{error}</p>{/if}
      <div class="modal-actions"><button class="btn primary" onclick={finish}>Finish</button></div>
    {:else}
      <p class="message error">{error}</p>
      {#if usedKey}<p class="small">The key file is {usedKey.path}</p>{/if}
      <div class="modal-actions">
        <button class="btn" onclick={onclose}>Close</button>
        <button class="btn primary" onclick={() => (stage = "choose")}>Try again</button>
      </div>
    {/if}

    {#if question}
      <PromptDialog prompt={question.prompt} onanswer={answer} />
    {/if}
  </div>
</div>

<style>
  .path {
    word-break: break-all;
    font: 12px var(--mono);
  }
  .indent {
    margin-left: 24px;
  }
  .small {
    color: var(--text-faint);
    font-size: 12px;
    word-break: break-all;
  }
</style>
