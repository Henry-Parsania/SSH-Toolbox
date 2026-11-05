<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import {
    allThemes,
    deleteCustomTheme,
    newThemeId,
    saveCustomTheme,
    setPreview,
  } from "./themes.svelte";
  import { COLOR_LABELS, isHex, terminalThemeFor, themeVariables, warnings, type AppTheme } from "./themes";
  import type { TerminalTheme } from "../terminal/themes";

  interface Props {
    /** The theme the settings window has picked right now. */
    current: string;
    /** Called when the person presses "Use this theme". */
    onuse: (id: string) => void;
    onclose: () => void;
  }

  let { current, onuse, onclose }: Props = $props();

  const ANSI: (keyof TerminalTheme)[] = ["red", "green", "yellow", "blue", "magenta", "cyan"];

  const clone = (t: AppTheme): AppTheme => structuredClone($state.snapshot(t));

  const start = untrack(() => current);
  let list = $state(allThemes());
  let selected = $state(start);
  let draft = $state<AppTheme>(clone(allThemes().find((t) => t.id === start) ?? allThemes()[0]));
  let note = $state("");

  const locked = $derived(!!draft.builtin);
  const vars = $derived(
    Object.entries(themeVariables(draft))
      .map(([k, v]) => `${k}:${v}`)
      .join(";"),
  );
  const term = $derived(terminalThemeFor(draft));
  const problems = $derived(warnings(draft));

  // Every change shows in the whole window straight away, saved or not.
  $effect(() => {
    setPreview(draft);
  });
  onDestroy(() => setPreview(null));

  function refresh(pick: string) {
    list = allThemes();
    pickTheme(pick);
  }

  function pickTheme(id: string) {
    const found = list.find((t) => t.id === id) ?? list[0];
    selected = found.id;
    draft = clone(found);
    note = "";
  }

  /** A copy of the theme on screen, which can be changed (the built-in ones can't). */
  function copyTheme() {
    const copy = clone(draft);
    copy.id = newThemeId();
    copy.name = `${draft.name} copy`.slice(0, 40);
    copy.builtin = undefined;
    saveCustomTheme(copy);
    refresh(copy.id);
    note = "Copied. Change the colours, then press Save.";
  }

  function save() {
    if (locked) return;
    if (!draft.name.trim()) {
      note = "Give the theme a name.";
      return;
    }
    saveCustomTheme({ ...clone(draft), name: draft.name.trim() });
    refresh(draft.id);
    note = "Saved.";
  }

  function remove() {
    if (locked) return;
    deleteCustomTheme(draft.id);
    refresh("dark");
  }

  function setColor(key: keyof AppTheme["colors"], value: string) {
    if (!locked && isHex(value)) draft.colors[key] = value;
  }

  function setAnsi(key: keyof TerminalTheme, value: string) {
    if (locked || !isHex(value)) return;
    draft.terminal = { ...(draft.terminal ?? {}), [key]: value };
  }
</script>

<div class="dock">
  <div
    class="modal builder"
    role="dialog"
    aria-label="Themes"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>Themes</h2>
    <p class="lead">Changes show in the window behind as you make them.</p>

    <div class="top">
      <label class="field grow">
        Theme
        <select value={selected} onchange={(e) => pickTheme(e.currentTarget.value)}>
          <optgroup label="Built in">
            {#each list.filter((t) => t.builtin) as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
          </optgroup>
          {#if list.some((t) => !t.builtin)}
            <optgroup label="Yours">
              {#each list.filter((t) => !t.builtin) as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
            </optgroup>
          {/if}
        </select>
      </label>
      <button class="btn" onclick={copyTheme} title="Make your own theme, starting from this one">New from this</button>
    </div>

    {#if locked}
      <p class="message warn">This is a built-in theme and can't be changed. Press "New from this" to make your own.</p>
    {:else}
      <label class="field">
        Name
        <input type="text" bind:value={draft.name} maxlength="40" />
      </label>
    {/if}

    <div class="grid">
      {#each COLOR_LABELS as c (c.key)}
        <div class="pick">
          <input
            type="color"
            disabled={locked}
            value={draft.colors[c.key]}
            oninput={(e) => setColor(c.key, e.currentTarget.value)}
            aria-label={c.label}
          />
          <span class="what"><b>{c.label}</b><small>{c.hint}</small></span>
          <input
            class="hex"
            type="text"
            disabled={locked}
            value={draft.colors[c.key]}
            maxlength="7"
            spellcheck="false"
            onchange={(e) => setColor(c.key, e.currentTarget.value.trim())}
            aria-label="{c.label} as a hex code"
          />
        </div>
      {/each}
    </div>

    <label class="field">
      Mostly
      <div class="seg" role="radiogroup" aria-label="Mostly">
        <button type="button" role="radio" aria-checked={draft.base === "dark"} class:picked={draft.base === "dark"} disabled={locked} onclick={() => (draft.base = "dark")}>Dark</button>
        <button type="button" role="radio" aria-checked={draft.base === "light"} class:picked={draft.base === "light"} disabled={locked} onclick={() => (draft.base = "light")}>Light</button>
      </div>
      <small>Decides the warning colour and the shades of grey in the terminal.</small>
    </label>

    <details>
      <summary>Terminal colours (optional)</summary>
      <p class="lead2">Worked out from the colours above. Change any of them to be different in the terminal only.</p>
      <div class="grid ansi">
        {#each ANSI as k (k)}
          <div class="pick">
            <input type="color" disabled={locked} value={term[k]} oninput={(e) => setAnsi(k, e.currentTarget.value)} aria-label={k} />
            <span class="what"><b>{k[0].toUpperCase() + k.slice(1)}</b></span>
          </div>
        {/each}
      </div>
      {#if !locked && draft.terminal && Object.keys(draft.terminal).length}
        <button class="btn small ghost" onclick={() => (draft.terminal = {})}>Back to the worked-out colours</button>
      {/if}
    </details>

    {#each problems as p (p)}<p class="message warn">{p}</p>{/each}

    <div class="preview" style={vars}>
      <div class="p-panel">
        <span class="p-title">Sessions</span>
        <span class="p-muted">user@server.example</span>
        <div class="p-buttons">
          <button class="btn small go" tabindex="-1">Connect</button>
          <button class="btn small primary" tabindex="-1">Upload</button>
          <button class="btn small danger" tabindex="-1">Delete</button>
        </div>
      </div>
      <div class="p-term" style:background={term.background} style:color={term.foreground}>
        <div><span style:color={term.green}>me@server</span>:<span style:color={term.blue}>~</span>$ ls</div>
        <div><span style:color={term.blue}>docs</span> notes.txt <span style:color={term.red}>error.log</span> <span style:color={term.yellow}>todo</span></div>
        <div><span class="cursor" style:background={term.cursor}></span></div>
      </div>
    </div>

    {#if note}<p class="message ok">{note}</p>{/if}

    <div class="modal-actions">
      {#if !locked}<button class="btn danger" onclick={remove}>Delete</button>{/if}
      <span class="spacer"></span>
      <button class="btn" onclick={onclose}>Close</button>
      {#if !locked}<button class="btn" onclick={save}>Save</button>{/if}
      <button class="btn primary" onclick={() => { if (!locked) save(); onuse(selected); }}>Use this theme</button>
    </div>
  </div>
</div>

<style>
  /* A side panel that leaves the app visible and undimmed, so changes can be seen. */
  .dock {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    padding: 12px;
    pointer-events: none;
  }
  .builder {
    width: 440px;
    max-height: calc(100vh - 24px);
    pointer-events: auto;
  }
  .top {
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }
  .top .btn {
    margin-bottom: 12px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 14px;
    margin-bottom: 12px;
  }
  .grid.ansi {
    grid-template-columns: 1fr 1fr 1fr;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pick input[type="color"] {
    flex: none;
    width: 34px;
    height: 34px;
    padding: 0;
    background: none;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    cursor: pointer;
  }
  .what {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    line-height: 1.2;
  }
  .what small {
    color: var(--text-faint);
    font-size: 11px;
  }
  .hex {
    width: 82px;
    padding: 4px 6px;
    font: 12px var(--mono);
  }
  details {
    margin-bottom: 12px;
    color: var(--text-muted);
  }
  summary {
    cursor: pointer;
    margin-bottom: 8px;
  }
  .lead2 {
    margin: 0 0 8px;
    font-size: 12px;
  }
  .preview {
    display: flex;
    gap: 10px;
    margin: 12px 0;
    padding: 12px;
    color: var(--text);
    background: var(--bg-0);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
  }
  .p-panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: none;
    width: 46%;
    padding: 10px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
  }
  .p-title {
    font-weight: 600;
  }
  .p-muted {
    color: var(--text-muted);
    font-size: 12px;
  }
  .p-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .p-term {
    flex: 1;
    min-width: 0;
    padding: 10px;
    font: 12px/1.5 var(--mono);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
  }
  .cursor {
    display: inline-block;
    width: 8px;
    height: 14px;
    vertical-align: middle;
  }
  .spacer {
    flex: 1;
  }
  .modal-actions {
    align-items: center;
  }
</style>
