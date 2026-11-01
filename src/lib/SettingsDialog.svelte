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

<div class="modal-backdrop">
  <div
    class="modal"
    role="dialog"
    aria-label="Settings"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>Settings</h2>

    <label class="field">
      Font
      <input type="text" bind:value={draft.font_family} />
    </label>
    <label class="field">
      Font size
      <input type="number" min="8" max="48" bind:value={draft.font_size} />
    </label>
    <label class="field">
      Colour theme
      <select bind:value={draft.theme}>
        <option value="dark">Dark</option>
        <option value="light">Light</option>
      </select>
    </label>
    <label class="field">
      Scrollback (lines, at least 10,000)
      <input type="number" min="10000" max="1000000" step="1000" bind:value={draft.scrollback} />
    </label>
    <label class="field">
      Terminal drawing
      <select bind:value={draft.renderer}>
        <option value="auto">Automatic (WebGL on Windows, basic on Linux)</option>
        <option value="dom">Basic (DOM)</option>
        <option value="webgl">WebGL (faster, if it works)</option>
      </select>
      <small>Applies to terminals opened after saving.</small>
    </label>
    {#if platform.os === "linux"}
      <label class="field">
        Display workaround
        <select bind:value={draft.webkit_workaround}>
          <option value="off">Off</option>
          <option value="disable_dmabuf">Disable DMABUF renderer</option>
          <option value="disable_compositing">Disable compositing</option>
        </select>
        <small>Only if the window shows blank or garbled. Takes effect after a restart.</small>
      </label>
    {/if}
    <label class="field">
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

    {#if error}<p class="message error">{error}</p>{/if}
    <div class="modal-actions">
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" disabled={saving} onclick={save}>Save</button>
    </div>
  </div>
</div>
