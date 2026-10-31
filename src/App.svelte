<script lang="ts">
  import {
    listShells,
    platformInfo,
    terminalAck,
    terminalClose,
    terminalOpen,
    terminalWrite,
    type PlatformInfo,
    type Shell,
    type TerminalStatus,
  } from "./lib/backend";

  let info = $state<PlatformInfo | null>(null);
  let failure = $state<string | null>(null);

  $effect(() => {
    platformInfo().then(
      (result) => (info = result),
      (err: { message?: string }) => (failure = err.message ?? "Couldn't read system details."),
    );
  });

  // TEMPORARY, until task 1.4 puts a real terminal here.
  let shells = $state<Shell[] | null>(null);
  let trial = $state("");

  async function showShells() {
    shells = await listShells();
  }

  async function tryShell() {
    const decoder = new TextDecoder();
    const encoder = new TextEncoder();
    let text = "";
    let statuses: TerminalStatus[] = [];
    let id = "";
    let answered = false;
    id = await terminalOpen(
      { type: "local", shell_id: shells?.[0]?.id ?? "" },
      80,
      24,
      (chunk) => {
        text += decoder.decode(chunk, { stream: true });
        if (!answered && text.includes("\x1b[6n")) {
          answered = true;
          terminalWrite(id, encoder.encode("\x1b[1;1R"));
        }
        terminalAck(id, chunk.length);
      },
      (status) => (statuses = [...statuses, status]),
    );
    await terminalWrite(id, encoder.encode("echo toolbox-trial\r\nexit\r\n"));
    await new Promise((resolve) => setTimeout(resolve, 3000));
    await terminalClose(id);
    trial = JSON.stringify({ sawEcho: text.includes("toolbox-trial"), statuses });
  }
</script>

<main>
  <h1>SSH Toolbox</h1>
  {#if failure}
    <p class="error">{failure}</p>
  {:else if info}
    <p>Running on <strong>{info.os}</strong></p>
  {:else}
    <p>Loading…</p>
  {/if}

  <p>
    <button id="shells" onclick={showShells}>List shells</button>
    {#if shells}<span id="shell-list">{shells.map((s) => s.name).join(", ")}</span>{/if}
  </p>
  <p>
    <button id="trial" onclick={tryShell}>Try a shell</button>
    <span id="trial-result">{trial}</span>
  </p>
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    padding: 2rem;
  }
  .error {
    color: #b00020;
  }
</style>
