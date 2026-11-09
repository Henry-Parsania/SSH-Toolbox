<script lang="ts">
  // The agent's behaviour: its instructions, its limits, how careful it must be, and some plain
  // information on how it works. Shown as tabs of the agent settings window.
  import { onMount } from "svelte";
  import { agentSettingsGet, agentSettingsSet, type AgentSettings } from "../backend/agent";
  import { DEFAULT_RULES } from "./agent.svelte";

  interface Props {
    tab: "instructions" | "safety" | "info";
  }

  let { tab }: Props = $props();

  const DEFAULTS: AgentSettings = { system_prompt: "", extra_instructions: "", max_rounds: 15, token_cap: 400000, allow_dangerous: false };

  let saved = $state<AgentSettings>({ ...DEFAULTS });
  /** The instructions text on screen. It shows the built-in text when nothing is replaced. */
  let prompt = $state(DEFAULT_RULES);
  let extra = $state("");
  let rounds = $state(DEFAULTS.max_rounds);
  let cap = $state(DEFAULTS.token_cap);
  let dangerous = $state(false);
  let typed = $state("");
  let note = $state("");
  let error = $state("");
  let busy = $state(false);

  onMount(async () => {
    const got = await agentSettingsGet().catch(() => DEFAULTS);
    saved = got;
    prompt = got.system_prompt || DEFAULT_RULES;
    extra = got.extra_instructions;
    rounds = got.max_rounds;
    cap = got.token_cap;
    dangerous = got.allow_dangerous;
  });

  const customPrompt = $derived(prompt.trim() !== DEFAULT_RULES.trim());
  const dirty = $derived(
    (customPrompt ? prompt : "") !== saved.system_prompt ||
      extra !== saved.extra_instructions ||
      Number(rounds) !== saved.max_rounds ||
      Number(cap) !== saved.token_cap ||
      dangerous !== saved.allow_dangerous,
  );
  const canTurnOn = $derived(typed.trim().toLowerCase() === "i understand");

  async function save() {
    busy = true;
    error = "";
    note = "";
    try {
      const next: AgentSettings = {
        // Keeping the built-in text as "empty" lets future improvements to it reach this user.
        system_prompt: customPrompt ? prompt : "",
        extra_instructions: extra,
        max_rounds: Number(rounds),
        token_cap: Number(cap),
        allow_dangerous: dangerous,
      };
      await agentSettingsSet(next);
      saved = next;
      note = "Saved. It applies from the agent's next message.";
    } catch (e) {
      error = (e as { message?: string }).message ?? "Couldn't save.";
    } finally {
      busy = false;
    }
  }
</script>

<div class="options">
  {#if tab === "instructions"}
    <p class="lead">
      The agent is a language model, so what it does depends on the instructions it is given. These are sent with every
      request. Change them to suit how you work.
    </p>
    <label class="field">
      Instructions
      <textarea rows="12" bind:value={prompt} spellcheck="false"></textarea>
      <small>
        The app always adds where you are connected (the server's name, and whether commands go through your terminal)
        after this text. {customPrompt ? "You have changed the built-in text." : "This is the built-in text."}
      </small>
    </label>
    <div class="line">
      <button class="btn small" disabled={!customPrompt} onclick={() => (prompt = DEFAULT_RULES)}>Reset to the built-in text</button>
    </div>
    <label class="field">
      Extra instructions
      <textarea rows="3" bind:value={extra} placeholder="For example: Answer in German. This is a production server, never restart services." spellcheck="false"></textarea>
      <small>Added after the instructions. Handy for preferences that you don't want to mix into the main text.</small>
    </label>
    <div class="row">
      <label class="field grow">
        Steps per task
        <input type="number" min="1" max="100" bind:value={rounds} />
        <small>How many times the agent may go back to the model before it stops and asks you to say "continue". Built-in: 15.</small>
      </label>
      <label class="field grow">
        Token limit per task
        <input type="number" min="10000" max="5000000" step="10000" bind:value={cap} />
        <small>A cost guard. Built-in: 400,000.</small>
      </label>
    </div>
  {:else if tab === "safety"}
    <p class="lead">
      Safety is decided by the app, not by the model, so no instruction or clever wording can switch it off. These are the
      rules, and the one switch you can change.
    </p>
    <h3>Always on</h3>
    <ul class="rules">
      <li>Anything that changes something (a command, a file write, typing into your terminal) is shown to you first, exactly as the model sent it, and waits for your approval.</li>
      <li>Passwords, keys and tokens are replaced with [hidden] before anything reaches the model.</li>
      <li>Files that may hold secrets (keys, <code>.env</code>, <code>/etc/shadow</code>) need your approval to read.</li>
      <li>Only a short list of looking-only commands (ls, df, cat, ps, grep...) can ever run without asking, and only if you tick the box in the agent panel.</li>
    </ul>
    <h3>Dangerous commands</h3>
    <p>
      Some commands are normally blocked outright, even if you approve them: wiping a disk, <code>rm -rf /</code>,
      fork bombs and the like. If you really need the agent to be able to propose them (for example to wipe a test
      machine you are about to throw away), you can allow them here.
    </p>
    <div class="warn">
      <strong>Be careful.</strong> A model can be wrong, or be tricked by text it reads. When this is on, every such
      command still appears with a red "Dangerous" label and runs only if you press "Run anyway", one command at a time,
      never automatically. There is no undo.
    </div>
    {#if !saved.allow_dangerous && !dangerous}
      <label class="field">
        To turn it on, type <b>I understand</b>
        <input type="text" bind:value={typed} autocomplete="off" spellcheck="false" />
      </label>
    {/if}
    <label class="check">
      <input type="checkbox" checked={dangerous} disabled={!dangerous && !canTurnOn} onchange={(e) => { dangerous = e.currentTarget.checked; if (!dangerous) typed = ""; }} />
      Allow the agent to propose normally blocked (dangerous) commands
    </label>
  {:else}
    <h3>How it works</h3>
    <p>
      You write a request in the agent panel. The app sends it to the model you chose (set up on the Providers tab),
      together with the instructions and a short list of tools. The model answers with text, or asks to use a tool. The
      app runs the tool, sends the result back, and repeats until the task is done, you stop it, or a limit is reached.
    </p>
    <h3>The tools</h3>
    <ul class="rules">
      <li><b>Run a command</b> on the server of the active SSH tab (30 seconds, no interactive programs). Or, if you ticked "use my terminal", type it into your own visible terminal.</li>
      <li><b>Read a file</b> (up to 256 KB) and <b>list a folder</b> on the server.</li>
      <li><b>Write a file</b>: replaces the whole content, after you see the changes and approve them.</li>
      <li><b>Read your terminal</b> screen, and <b>type into it</b> after your approval.</li>
    </ul>
    <h3>What leaves this computer</h3>
    <p>
      Your messages, and what the agent reads (terminal text, files, command output), go to the provider you chose, with
      secrets removed first. With a local model (Ollama) nothing leaves your computer. Your API key is kept in the system
      keychain and is only ever sent to that provider's address.
    </p>
    <h3>Good to know</h3>
    <ul class="rules">
      <li>A model is not deterministic: the same request can give different results. Small local models are often unreliable with tools.</li>
      <li>Read what you approve. The approval box shows the exact command or the exact changes.</li>
      <li>These settings are stored in <code>agent_settings.json</code> in the app's settings folder. Deleting that file restores the built-in behaviour.</li>
    </ul>
  {/if}

  {#if tab !== "info"}
    {#if note}<p class="message ok">{note}</p>{/if}
    {#if error}<p class="message error">{error}</p>{/if}
    <div class="save">
      <button class="btn primary" disabled={busy || !dirty} onclick={save}>Save</button>
    </div>
  {/if}
</div>

<style>
  .options {
    min-height: 260px;
  }
  textarea {
    font: 12px/1.45 var(--mono);
    resize: vertical;
  }
  .line {
    margin: -4px 0 12px;
  }
  .rules {
    margin: 0 0 10px;
    padding-left: 18px;
  }
  .rules li {
    margin-bottom: 4px;
  }
  code {
    font: 12px var(--mono);
    padding: 0 4px;
    background: var(--bg-2);
    border-radius: 4px;
  }
  .warn {
    margin: 8px 0 12px;
    padding: 10px 12px;
    color: var(--danger);
    background: var(--danger-soft);
    border: 1px solid var(--danger);
    border-radius: var(--radius-s);
  }
  .save {
    display: flex;
    justify-content: flex-end;
    padding-top: 4px;
  }
</style>
