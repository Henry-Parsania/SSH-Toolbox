<script lang="ts">
  // Where the agent's model comes from: Claude, or any service that speaks the OpenAI chat format.
  import { onMount } from "svelte";
  import {
    agentComplete,
    agentProviderDelete,
    agentProviders,
    agentProviderSave,
    type Provider,
    type ProviderKind,
    type ProviderView,
  } from "../backend/agent";
  import Icon from "../ui/Icon.svelte";
  import AgentOptions from "./AgentOptions.svelte";

  interface Props {
    onclose: () => void;
    /** Called after anything is saved or deleted, with the new list. */
    onchanged: (providers: ProviderView[]) => void;
  }

  let { onclose, onchanged }: Props = $props();

  // Starting points. The model names change often, so they can be edited.
  const presets: { name: string; kind: ProviderKind; base_url: string; model: string; note?: string }[] = [
    { name: "Claude (Anthropic)", kind: "anthropic", base_url: "https://api.anthropic.com", model: "claude-sonnet-5-5" },
    { name: "OpenAI", kind: "open_ai_compatible", base_url: "https://api.openai.com/v1", model: "gpt-4.1" },
    { name: "DeepSeek", kind: "open_ai_compatible", base_url: "https://api.deepseek.com", model: "deepseek-chat", note: "Servers in China." },
    { name: "Qwen (Alibaba)", kind: "open_ai_compatible", base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", model: "qwen-plus", note: "Servers in China or Singapore." },
    { name: "OpenRouter", kind: "open_ai_compatible", base_url: "https://openrouter.ai/api/v1", model: "anthropic/claude-sonnet-5-5" },
    { name: "Ollama (this computer)", kind: "open_ai_compatible", base_url: "http://localhost:11434/v1", model: "qwen2.5:7b", note: "Nothing leaves this computer. Small models are often unreliable with tools." },
    { name: "Other", kind: "open_ai_compatible", base_url: "https://", model: "" },
  ];

  let tab = $state<"providers" | "instructions" | "safety" | "info">("providers");
  let providers = $state<ProviderView[]>([]);
  let form = $state<(Provider & { hasKey: boolean; forNow: boolean }) | null>(null);
  let apiKey = $state("");
  let note = $state("");
  let error = $state("");
  let busy = $state(false);

  onMount(() => void reload());

  async function reload() {
    providers = await agentProviders().catch(() => []);
    onchanged(providers);
  }

  function edit(p: ProviderView) {
    form = { ...p, hasKey: p.has_key, forNow: p.key_for_now_only };
    apiKey = "";
    note = "";
    error = "";
  }

  function add(preset: (typeof presets)[number]) {
    form = { id: "", name: preset.name, kind: preset.kind, base_url: preset.base_url, model: preset.model, hasKey: false, forNow: false };
    apiKey = "";
    note = preset.note ?? "";
    error = "";
  }

  async function save(): Promise<ProviderView | null> {
    if (!form) return null;
    busy = true;
    error = "";
    try {
      const { hasKey: _a, forNow: _b, ...provider } = form;
      const saved = await agentProviderSave(provider, apiKey);
      apiKey = "";
      form = { ...saved, hasKey: saved.has_key, forNow: saved.key_for_now_only };
      note = saved.key_for_now_only ? "The system keychain isn't available, so the key is kept only until the app closes." : "Saved.";
      await reload();
      return saved;
    } catch (e) {
      error = (e as { message?: string }).message ?? "Couldn't save.";
      return null;
    } finally {
      busy = false;
    }
  }

  async function test() {
    const saved = await save();
    if (!saved) return;
    busy = true;
    note = "Asking the model…";
    try {
      const reply = await agentComplete(crypto.randomUUID(), saved.id, "Answer very briefly.", [{ role: "user", text: "Reply with the single word OK." }], []);
      note = `It works. The model said: ${reply.text.trim().slice(0, 80) || "(nothing)"}`;
    } catch (e) {
      note = "";
      error = (e as { message?: string }).message ?? "The test failed.";
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!form?.id) {
      form = null;
      return;
    }
    await agentProviderDelete(form.id).catch(() => {});
    form = null;
    await reload();
  }

  const host = $derived.by(() => {
    try {
      return new URL(form?.base_url ?? "").host;
    } catch {
      return "";
    }
  });
</script>

<div class="modal-backdrop">
  <div class="modal wide" role="dialog" aria-label="Agent settings" tabindex="-1" onkeydown={(e) => e.key === "Escape" && onclose()}>
    <h2>Agent settings</h2>
    <div class="seg" role="tablist" aria-label="Agent settings">
      {#each [["providers", "Providers"], ["instructions", "Instructions"], ["safety", "Safety"], ["info", "How it works"]] as [id, label] (id)}
        <button type="button" role="tab" aria-selected={tab === id} class:picked={tab === id} onclick={() => (tab = id as typeof tab)}>{label}</button>
      {/each}
    </div>

    {#if tab === "providers"}
    <p class="lead">Pick the service that powers the agent. Anything the agent reads (terminal text, files, command output) is sent to it, with passwords and keys removed first.</p>

    <div class="cols">
      <div class="left">
        {#each providers as p (p.id)}
          <button class="item" class:on={form?.id === p.id} onclick={() => edit(p)}>
            <span>{p.name}</span>
            <small>{p.model}{p.has_key ? "" : " · no key"}</small>
          </button>
        {/each}
        {#if providers.length === 0}<p class="hint">Nothing set up yet. Add one:</p>{/if}
        <div class="adds">
          {#each presets as preset (preset.name)}
            <button class="btn small" onclick={() => add(preset)}><Icon name="plus" size={12} /> {preset.name}</button>
          {/each}
        </div>
      </div>

      <div class="right">
        {#if form}
          <label class="field">
            Name
            <input type="text" bind:value={form.name} />
          </label>
          <label class="field">
            Kind
            <select bind:value={form.kind}>
              <option value="anthropic">Claude (Anthropic's own format)</option>
              <option value="open_ai_compatible">OpenAI-compatible (most others)</option>
            </select>
          </label>
          <label class="field">
            Address
            <input type="text" bind:value={form.base_url} spellcheck="false" />
            <small>{#if host}Your data goes to {host}.{/if}</small>
          </label>
          <label class="field">
            Model
            <input type="text" bind:value={form.model} spellcheck="false" />
          </label>
          <label class="field">
            API key
            <input
              type="password"
              bind:value={apiKey}
              autocomplete="off"
              placeholder={form.hasKey ? "Saved. Leave empty to keep it." : form.base_url.includes("localhost") ? "Not needed for a model on this computer" : "Paste the key"}
            />
            <small>Kept in the system keychain. It is sent only to the address above and is never shown again.</small>
          </label>
          {#if note}<p class="message note">{note}</p>{/if}
          {#if error}<p class="message error">{error}</p>{/if}
          <div class="row-actions">
            <button class="btn danger" disabled={busy} onclick={remove}>{form.id ? "Delete" : "Discard"}</button>
            <span class="spacer"></span>
            <button class="btn" disabled={busy} onclick={save}>Save</button>
            <button class="btn primary" disabled={busy || !form.model.trim()} onclick={test}>Save and test</button>
          </div>
        {:else}
          <p class="hint">Choose a provider on the left, or add one.</p>
        {/if}
      </div>
    </div>
    {:else}
      <AgentOptions {tab} />
    {/if}

    <div class="modal-actions">
      <button class="btn" onclick={onclose}>Close</button>
    </div>
  </div>
</div>

<style>
  .seg {
    margin: 0 0 14px;
  }
  .cols {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 16px;
    margin: 8px 0;
  }
  .left {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .item {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 6px 10px;
    color: var(--text);
    text-align: left;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    cursor: pointer;
  }
  .item.on {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .item small {
    color: var(--text-muted);
  }
  .adds {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 8px;
  }
  .row-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }
  .spacer {
    flex: 1;
  }
  .hint {
    color: var(--text-muted);
    font-size: 12px;
  }
  .note {
    color: var(--accent);
  }
</style>
