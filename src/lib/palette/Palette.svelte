<script lang="ts">
  // Ctrl+Shift+P: type to jump to a saved session, an open tab or a menu action. Text such as
  // pi@10.0.0.5:22 offers to connect straight to that address.

  import { onMount, tick } from "svelte";
  import { matchesQuery } from "./parse";

  export interface PaletteItem {
    /** What it is: "Session", "Tab" or the name of a menu. */
    group: string;
    label: string;
    hint?: string;
    run: () => void;
  }

  interface Props {
    items: PaletteItem[];
    /** An extra item built from what was typed, such as "Connect to pi@host". */
    extra: (query: string) => PaletteItem | null;
    onclose: () => void;
  }

  let { items, extra, onclose }: Props = $props();

  let query = $state("");
  let index = $state(0);
  let list = $state<HTMLElement>();
  let box = $state<HTMLInputElement>();

  // The terminal behind has the keyboard when the palette opens, so the box must take it.
  onMount(() => {
    box?.focus();
    const timer = setTimeout(() => box?.focus(), 50);
    return () => clearTimeout(timer);
  });

  const shown = $derived.by(() => {
    const found = items.filter((i) => matchesQuery(query, `${i.group} ${i.label}`)).slice(0, 60);
    const more = extra(query);
    return more ? [more, ...found] : found;
  });
  $effect(() => {
    void shown.length;
    if (index >= shown.length) index = Math.max(0, shown.length - 1);
  });

  function choose(item: PaletteItem | undefined) {
    if (!item) return;
    onclose();
    item.run();
  }

  async function move(by: number) {
    if (shown.length === 0) return;
    index = (index + by + shown.length) % shown.length;
    await tick();
    list?.querySelector(".on")?.scrollIntoView({ block: "nearest" });
  }
</script>

<div class="modal-backdrop top" onpointerdown={(e) => e.target === e.currentTarget && onclose()} role="presentation">
  <div class="palette" role="dialog" aria-label="Command palette" tabindex="-1">
    <input
      type="text"
      bind:this={box}
      placeholder="Type a session, a tab, a command, or user@host:port"
      bind:value={query}
      oninput={() => (index = 0)}
      onkeydown={(e) => {
        if (e.key === "Escape") onclose();
        else if (e.key === "ArrowDown") void move(1);
        else if (e.key === "ArrowUp") void move(-1);
        else if (e.key === "Enter") choose(shown[index]);
        else return;
        e.preventDefault();
        e.stopPropagation();
      }}
    />
    <ul bind:this={list} role="listbox">
      {#each shown as item, i (item.group + "\u0000" + item.label + "\u0000" + i)}
        <li role="option" aria-selected={i === index} class:on={i === index}>
          <button tabindex="-1" onpointermove={() => (index = i)} onclick={() => choose(item)}>
            <span class="group">{item.group}</span>
            <span class="label">{item.label}</span>
            {#if item.hint}<span class="hint">{item.hint}</span>{/if}
          </button>
        </li>
      {:else}
        <li class="none">Nothing matches.</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .top {
    align-items: flex-start;
    padding-top: 12vh;
  }
  .palette {
    width: min(620px, 92vw);
    overflow: hidden;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.5);
  }
  input {
    width: 100%;
    padding: 13px 16px;
    color: var(--text);
    font-size: 14px;
    background: transparent;
    border: 0;
    border-bottom: 1px solid var(--border);
    outline: none;
  }
  ul {
    max-height: 50vh;
    margin: 0;
    padding: 4px;
    overflow: auto;
    list-style: none;
  }
  li button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    color: var(--text);
    text-align: left;
    background: transparent;
    border: 0;
    border-radius: 6px;
    cursor: default;
  }
  li.on button {
    background: var(--bg-hover);
  }
  .group {
    flex: none;
    min-width: 64px;
    color: var(--text-faint);
    font-size: 11px;
    text-transform: uppercase;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    color: var(--text-faint);
    font: 11px var(--mono);
  }
  .none {
    padding: 12px;
    color: var(--text-faint);
  }
</style>
