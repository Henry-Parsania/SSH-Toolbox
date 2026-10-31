<script lang="ts">
  import type { PlatformInfo, Settings, Shell } from "./backend";
  import { saveSettings, settings } from "./settings.svelte";

  interface Props {
    platform: PlatformInfo;
    shells: Shell[];
    onclose: () => void;
  }

  let { platform, shells, onclose }: Props = $props();

  let draft = $state<Settings>({ ...settings });
  let error = $state("");
  let saving = $state(false);

  async function save() {
    saving = true;
    error = "";
    try {
      await saveSettings({ ...draft, font_size: Number(draft.font_size), scrollback: Number(draft.scrollback) });
      onclose();
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't save the settings.";
    } finally {
      saving = false;
    }
  }
</script>

<div class="backdrop">
  <div
    class="dialog"
    role="dialog"
    aria-label="Settings"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>Settings</h2>

    <label>
      Font
      <input type="text" bind:value={draft.font_family} />
    </label>
    <label>
      Font size
      <input type="number" min="8" max="48" bind:value={draft.font_size} />
    </label>
    <label>
      Colour theme
      <select bind:value={draft.theme}>
        <option value="dark">Dark</option>
        <option value="light">Light</option>
      </select>
    </label>
    <label>
      Scrollback (lines, at least 10,000)
      <input type="number" min="10000" max="1000000" step="1000" bind:value={draft.scrollback} />
    </label>
    <label>
      Terminal drawing
      <select bind:value={draft.renderer}>
        <option value="auto">Automatic (WebGL on Windows, basic on Linux)</option>
        <option value="dom">Basic (DOM)</option>
        <option value="webgl">WebGL (faster, if it works)</option>
      </select>
      <small>Applies to terminals opened after saving.</small>
    </label>
    {#if platform.os === "linux"}
      <label>
        Display workaround
        <select bind:value={draft.webkit_workaround}>
          <option value="off">Off</option>
          <option value="disable_dmabuf">Disable DMABUF renderer</option>
          <option value="disable_compositing">Disable compositing</option>
        </select>
        <small>Only if the window shows blank or garbled. Takes effect after a restart.</small>
      </label>
    {/if}
    <label>
      Shell for new tabs
      <select bind:value={draft.default_shell}>
        <option value="">First one found</option>
        {#each shells as shell (shell.id)}
          <option value={shell.id}>{shell.name}</option>
        {/each}
      </select>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={draft.restore_layout} />
      Bring back tabs and splits when the app starts
    </label>

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
    width: min(460px, 92vw);
    max-height: 90vh;
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
  input[type="text"],
  input[type="number"],
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
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
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
