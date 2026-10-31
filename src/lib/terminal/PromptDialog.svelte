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
    <button autofocus={warning} onclick={() => answer(false)}>Cancel</button>
    <button class="primary" onclick={() => answer(true)}>
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
    transform: translate(-50%, -50%);
    z-index: 5;
    width: min(460px, 90%);
    padding: 12px 16px;
    background: #252526;
    color: #d4d4d4;
    border: 1px solid #555;
    border-radius: 6px;
    font: 13px system-ui, sans-serif;
    box-shadow: 0 4px 16px #0008;
  }
  .box.warning {
    background: #4d2d00;
    border-color: #d18616;
  }
  .title {
    margin: 0 0 8px;
    font-weight: 600;
  }
  p {
    margin: 0 0 8px;
  }
  .mono {
    font: 12px Consolas, monospace;
    word-break: break-all;
    padding: 6px 8px;
    background: #1e1e1e;
  }
  input[type="password"] {
    display: block;
    width: 100%;
    box-sizing: border-box;
    padding: 4px 6px;
    margin-bottom: 8px;
    font: inherit;
    color: inherit;
    background: #3c3c3c;
    border: 1px solid #555;
  }
  .save {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-bottom: 8px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  button {
    padding: 4px 14px;
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
