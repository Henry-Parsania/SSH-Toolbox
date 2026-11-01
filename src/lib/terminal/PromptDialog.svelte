<script lang="ts">
  import type { Prompt, PromptResponse } from "../backend";

  interface Props {
    prompt: Prompt;
    onanswer: (response: PromptResponse) => void;
  }

  let { prompt, onanswer }: Props = $props();

  let text = $state("");
  let save = $state(false);

  const warning = $derived(prompt.kind === "host_key_new_type");
  const asksForText = $derived(prompt.kind === "password" || prompt.kind === "passphrase");
  const offerSave = $derived(
    (prompt.kind === "password" || prompt.kind === "passphrase") && prompt.offer_save,
  );

  function answer(accepted: boolean) {
    onanswer({
      accepted,
      text: asksForText && accepted ? text : undefined,
      save: offerSave && accepted ? save : false,
    });
    text = "";
  }
</script>

<div class="box" class:warning role="dialog" aria-label="Question">
  {#if prompt.kind === "host_key_unknown"}
    <p class="title">Trust this server?</p>
    <p>
      The first time you connect to <b>{prompt.host}</b> on port {prompt.port}, it shows this key.
      Check it matches what you expect.
    </p>
    <p class="mono">{prompt.algorithm}<br />{prompt.fingerprint}</p>
  {:else if prompt.kind === "host_key_new_type"}
    <p class="title">Warning: a different kind of key</p>
    <p>
      <b>{prompt.host}</b> (port {prompt.port}) is known by {prompt.known_algorithms.join(", ")},
      but is now offering a different type of key. This can happen after a server upgrade, but it
      can also be an attack.
    </p>
    <p class="mono">{prompt.algorithm}<br />{prompt.fingerprint}</p>
  {:else if prompt.kind === "password"}
    <p class="title">Password for {prompt.user}@{prompt.host}</p>
  {:else if prompt.kind === "passphrase"}
    <p class="title">Passphrase for the key</p>
    <p class="mono">{prompt.key_path}</p>
  {:else if prompt.kind === "ftps_certificate"}
    <p class="title">Trust this certificate?</p>
    <p>
      {prompt.host}: {prompt.reason}
    </p>
    <p class="mono">{prompt.subject}<br />{prompt.fingerprint}<br />Expires {prompt.expires}</p>
  {:else if prompt.kind === "plain_ftp"}
    <p class="title">Plain FTP isn't encrypted</p>
    <p>Everything sent to {prompt.host}, including the password, can be read on the way.</p>
  {/if}

  {#if asksForText}
    <!-- svelte-ignore a11y_autofocus -->
    <input
      type="password"
      autofocus
      bind:value={text}
      autocomplete="off"
      onkeydown={(e) => {
        if (e.key === "Enter") answer(true);
        else if (e.key === "Escape") answer(false);
        e.stopPropagation();
      }}
    />
  {/if}
  {#if offerSave}
    <label class="save"><input type="checkbox" bind:checked={save} /> Save in the keychain</label>
  {/if}

  <div class="buttons">
    <!-- Cancel is the default button when a key has changed type. -->
    <!-- svelte-ignore a11y_autofocus -->
    <button class="btn" autofocus={warning} onclick={() => answer(false)}>Cancel</button>
    <button class="btn primary" onclick={() => answer(true)}>
      {#if prompt.kind === "host_key_unknown" || prompt.kind === "host_key_new_type"}
        Trust and connect
      {:else if prompt.kind === "plain_ftp"}
        Connect anyway
      {:else}
        OK
      {/if}
    </button>
  </div>
</div>

<style>
  .box {
    position: absolute;
    top: 50%;
    left: 50%;
    z-index: 5;
    width: min(480px, 92%);
    padding: 18px 20px 16px;
    color: var(--text);
    background: var(--bg-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow);
    transform: translate(-50%, -50%);
    animation: pop-in 0.16s var(--ease);
  }
  .box.warning {
    background: var(--bg-1);
    border-color: var(--warning);
    box-shadow:
      0 0 0 1px var(--warning),
      var(--shadow);
  }
  .title {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
  }
  .warning .title {
    color: var(--warning);
  }
  p {
    margin: 0 0 10px;
    color: var(--text-muted);
  }
  .title {
    color: var(--text);
  }
  .warning .title {
    color: var(--warning);
  }
  .mono {
    padding: 9px 11px;
    color: var(--text);
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    font: 12px/1.5 var(--mono);
    word-break: break-all;
  }
  input[type="password"] {
    margin-bottom: 10px;
  }
  .save {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
</style>
