<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { serialListPorts, type PortInfo } from "../backend/serial";
  import type { Library } from "./library.svelte";
  import { folderChoices, newId, type SerialSession } from "./tree";

  interface Props {
    library: Library;
    /** The session being changed, or none for a new one. */
    editing?: SerialSession;
    /** The folder a new session starts in. */
    folder?: string;
    onclose: () => void;
  }

  let { library, editing, folder = "", onclose }: Props = $props();

  const BAUDS = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];

  const start = untrack(() => ({
    name: editing?.name ?? "",
    port: editing?.port ?? "",
    baud: editing?.baud ?? 115200,
    dataBits: editing?.data_bits ?? 8,
    parity: editing?.parity ?? ("none" as const),
    stopBits: editing?.stop_bits ?? 1,
    flow: editing?.flow_control ?? ("none" as const),
    echo: editing?.local_echo ?? false,
    ending: editing?.line_ending ?? ("cr" as const),
    folder: editing?.folder ?? folder,
  }));
  let name = $state(start.name);
  let port = $state(start.port);
  let baud = $state<string | number>(start.baud);
  let dataBits = $state(start.dataBits);
  let parity = $state<"none" | "odd" | "even">(start.parity);
  let stopBits = $state(start.stopBits);
  let flow = $state<"none" | "hardware" | "software">(start.flow);
  let echo = $state(start.echo);
  let ending = $state<"cr" | "lf" | "crlf">(start.ending);
  let inFolder = $state(start.folder);

  let ports = $state<PortInfo[]>([]);
  let listing = $state(false);
  let error = $state("");
  let saving = $state(false);

  const choices = $derived(folderChoices(library.file.folder));

  async function refresh() {
    listing = true;
    try {
      ports = await serialListPorts();
    } catch {
      ports = [];
    } finally {
      listing = false;
    }
  }
  onMount(() => void refresh());

  async function save() {
    if (!name.trim()) return void (error = "Give the session a name.");
    if (!port.trim()) return void (error = "Choose or type the port.");
    const speed = Number(baud);
    if (!Number.isInteger(speed) || speed < 50 || speed > 4_000_000) {
      return void (error = "Enter a baud rate between 50 and 4,000,000.");
    }
    saving = true;
    error = "";
    const id = editing?.id ?? newId();
    const session: SerialSession = {
      type: "serial",
      id,
      name: name.trim(),
      folder: inFolder,
      port: port.trim(),
      baud: speed,
      data_bits: Number(dataBits),
      parity,
      stop_bits: Number(stopBits),
      flow_control: flow,
      local_echo: echo,
      line_ending: ending,
    };
    const sessions = editing
      ? library.file.session.map((s) => (s.id === id ? session : s))
      : [...library.file.session, session];
    try {
      await library.save({ ...library.file, session: sessions });
      onclose();
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't save the session.";
    } finally {
      saving = false;
    }
  }
</script>

<div class="modal-backdrop">
  <div
    class="modal"
    role="dialog"
    aria-label="Serial session"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>{editing ? "Edit serial session" : "New serial session"}</h2>

    <label class="field">Name <input type="text" bind:value={name} /></label>
    <div class="row">
      <label class="field grow">
        Port
        <input type="text" list="serial-ports" bind:value={port} placeholder="COM3 or /dev/ttyUSB0" />
        <datalist id="serial-ports">
          {#each ports as p (p.name)}
            <option value={p.name}>{p.label}</option>
          {/each}
        </datalist>
      </label>
      <button class="btn small refresh" onclick={refresh} disabled={listing} title="Look for ports again">
        Refresh
      </button>
    </div>
    {#if ports.length > 0}
      <small class="found">
        Found: {ports.map((p) => (p.label ? `${p.name} (${p.label})` : p.name)).join(", ")}
      </small>
    {:else if !listing}
      <small class="found">No ports found right now. You can still type one.</small>
    {/if}

    <div class="row">
      <label class="field grow">
        Baud rate
        <input type="number" list="bauds" min="50" max="4000000" bind:value={baud} />
        <datalist id="bauds">
          {#each BAUDS as b (b)}<option value={b}></option>{/each}
        </datalist>
      </label>
      <label class="field">
        Data bits
        <select bind:value={dataBits}>
          {#each [5, 6, 7, 8] as b (b)}<option value={b}>{b}</option>{/each}
        </select>
      </label>
      <label class="field">
        Stop bits
        <select bind:value={stopBits}>
          <option value={1}>1</option>
          <option value={2}>2</option>
        </select>
      </label>
    </div>
    <div class="row">
      <label class="field grow">
        Parity
        <select bind:value={parity}>
          <option value="none">None</option>
          <option value="odd">Odd</option>
          <option value="even">Even</option>
        </select>
      </label>
      <label class="field grow">
        Flow control
        <select bind:value={flow}>
          <option value="none">None</option>
          <option value="hardware">Hardware (RTS/CTS)</option>
          <option value="software">Software (XON/XOFF)</option>
        </select>
      </label>
    </div>
    <div class="row">
      <label class="field grow">
        Enter key sends
        <select bind:value={ending}>
          <option value="cr">CR (carriage return)</option>
          <option value="lf">LF (line feed)</option>
          <option value="crlf">CR and LF</option>
        </select>
      </label>
      <label class="field grow">
        Folder
        <select bind:value={inFolder}>
          <option value="">(none)</option>
          {#each choices as choice (choice.id)}
            <option value={choice.id}>{choice.label}</option>
          {/each}
        </select>
      </label>
    </div>
    <label class="check">
      <input type="checkbox" bind:checked={echo} />
      Local echo (show what you type, for devices that don't send it back)
    </label>

    {#if error}<p class="message error">{error}</p>{/if}
    <div class="modal-actions">
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" disabled={saving} onclick={save}>Save</button>
    </div>
  </div>
</div>

<style>
  .refresh {
    flex: none;
    align-self: flex-end;
    margin-bottom: 14px;
  }
  .found {
    display: block;
    margin: -6px 0 10px;
    color: var(--text-muted);
  }
</style>
