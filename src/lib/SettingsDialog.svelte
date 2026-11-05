<script lang="ts">
  import type { PlatformInfo, Settings, Shell } from "./backend";
  import { saveSettings, settings } from "./settings.svelte";
  import { allThemes, chooseTheme, currentTheme } from "./ui/themes.svelte";
  import ThemeBuilder from "./ui/ThemeBuilder.svelte";

  interface Props {
    platform: PlatformInfo;
    shells: Shell[];
    onclose: () => void;
  }

  let { platform, shells, onclose }: Props = $props();

  let draft = $state<Settings>({ ...settings });
  let error = $state("");
  let saving = $state(false);
  let themeChoice = $state(currentTheme().id);
  let building = $state(false);

  async function save() {
    saving = true;
    error = "";
    try {
      const chosen = allThemes().find((t) => t.id === themeChoice) ?? currentTheme();
      await saveSettings({
        ...draft,
        theme: chosen.base,
        font_size: Number(draft.font_size),
        scrollback: Number(draft.scrollback),
        reachability_interval_secs: Number(draft.reachability_interval_secs),
        stats_interval_secs: Number(draft.stats_interval_secs),
      });
      chooseTheme(chosen.id);
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
    <div class="field">
      Colour theme
      <div class="theme-row">
        <select bind:value={themeChoice}>
          {#each allThemes() as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
        </select>
        <button type="button" class="btn" onclick={() => (building = true)}>Edit themes…</button>
      </div>
    </div>
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

    <h3 class="group">Window</h3>
    <label class="check">
      <input type="checkbox" bind:checked={draft.tray_icon} />
      Show an icon in the system tray (after a restart)
    </label>
    {#if draft.tray_icon}
      <label class="check">
        <input type="checkbox" bind:checked={draft.close_to_tray} />
        Closing the window keeps the app running in the tray
      </label>
    {/if}

    <h3 class="group">Connections</h3>
    <label class="check">
      <input type="checkbox" bind:checked={draft.auto_reconnect} />
      Reconnect an SSH session by itself when its connection drops (up to 5 tries)
    </label>

    <label class="field">
      Save a text copy of every terminal session in this folder (empty = don't)
      <input type="text" bind:value={draft.log_folder} placeholder="a folder for terminal logs" spellcheck="false" />
      <small>
        One file per session, with colours and cursor codes removed. A file holds everything the
        session showed, so keep the folder private. It applies to sessions opened after saving.
      </small>
    </label>

    <h3 class="group">Server status</h3>
    <label class="check">
      <input type="checkbox" bind:checked={draft.reachability_enabled} />
      Show which saved servers can be reached (a quick connection test)
    </label>
    {#if draft.reachability_enabled}
      <label class="field">
        Check again every (seconds, 0 = only at start and when the list changes)
        <input type="number" min="0" max="3600" bind:value={draft.reachability_interval_secs} />
      </label>
    {/if}
    <label class="check">
      <input type="checkbox" bind:checked={draft.stats_enabled} />
      Show CPU, memory and disk use in the footer
    </label>
    {#if draft.stats_enabled}
      <label class="field">
        Refresh the footer every (seconds)
        <input type="number" min="2" max="300" bind:value={draft.stats_interval_secs} />
      </label>
    {/if}

    {#if error}<p class="message error">{error}</p>{/if}
    <div class="modal-actions">
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" disabled={saving} onclick={save}>Save</button>
    </div>
  </div>
</div>

{#if building}
  <ThemeBuilder current={themeChoice} onuse={(id) => { themeChoice = id; building = false; }} onclose={() => (building = false)} />
{/if}

<style>
  .theme-row {
    display: flex;
    gap: 8px;
    margin-top: 5px;
  }
  .theme-row select {
    flex: 1;
  }
  .group {
    margin: 14px 0 4px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
</style>
