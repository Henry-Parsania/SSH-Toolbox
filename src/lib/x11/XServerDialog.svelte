<script lang="ts">
  // Shows whether windows from programs on a server can appear on this computer, and starts or
  // stops the X server the app uses for that.

  import { onMount } from "svelte";
  import { x11Start, x11Status, x11Stop, type X11Status } from "../backend/x11";

  interface Props {
    onclose: () => void;
  }

  let { onclose }: Props = $props();
  let status = $state<X11Status | null>(null);
  let busy = $state(false);
  let error = $state("");

  async function run(action: () => Promise<X11Status>) {
    busy = true;
    error = "";
    try {
      status = await action();
    } catch (e) {
      error = (e as { message?: string }).message ?? "That didn't work.";
      status = await x11Status().catch(() => status);
    } finally {
      busy = false;
    }
  }

  onMount(() => void run(x11Status));
</script>

<div class="modal-backdrop">
  <div
    class="modal"
    role="dialog"
    aria-label="X server"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>X server</h2>
    <p>
      Lets programs with windows, such as a text editor or a browser on a server, show them on this computer. Turn on
      "Show windows of programs on the server (X11)" in a session, and connect.
    </p>
    {#if status}
      <p class="state {status.state}"><strong>{status.state === "running" ? "Ready" : status.state === "stopped" ? "Not started" : "Not available"}.</strong> {status.detail}</p>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="modal-actions">
      <button class="btn" disabled={busy || status?.state !== "running"} onclick={() => run(x11Stop)}>Stop</button>
      <button class="btn" disabled={busy || status?.state === "running" || status?.state === "missing"} onclick={() => run(x11Start)}>Start</button>
      <button class="btn primary" onclick={onclose}>Close</button>
    </div>
  </div>
</div>

<style>
  .state.missing {
    color: var(--danger, #d9534f);
  }
</style>
