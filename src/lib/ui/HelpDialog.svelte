<script lang="ts">
  import { appVersion } from "../backend";
  // A list of what the app can do and how to reach it, so nothing gets forgotten.

  interface Props {
    /** Which page opens first. */
    page?: "features" | "shortcuts" | "about";
    onclose: () => void;
  }

  let { page = "features", onclose }: Props = $props();
  let version = $state("");
  void appVersion().then((v) => (version = v)).catch(() => {});
  let current = $state<"features" | "shortcuts" | "about">("features");
  $effect(() => {
    current = page;
  });

  const features: [string, string][] = [
    ["Terminals and tabs", "Local shells and WSL, in tabs that can be split side by side or top and bottom, as many times as you like. The arrow beside + opens any shell in a split, and right-clicking a saved session offers Open side by side and Open top and bottom. Tabs can be dragged into a new order and renamed. The last layout comes back next time. Terminal menu."],
    ["SSH sessions", "Saved in the sessions list, in folders, with password, key or agent login and optional jump hosts. A dot shows which servers answer. File menu."],
    ["Process monitor", "A button on each SSH session opens btop, htop or top (whichever the server has) in a pop-up window, to watch and manage processes. In the sessions list."],
    ["Key setup", "Make a key and put it on a server so passwords aren't needed. In the sessions list, on a session's key button."],
    ["Port forwarding", "Local, remote and SOCKS forwards for a session. In the sessions list, on a session's forward button."],
    ["Files", "Browse and copy between this computer and a server (SFTP, FTP or FTPS) by dragging. It can follow the folder the terminal is in. View menu."],
    ["Docker", "Containers, images, volumes and networks on this computer or on the SSH server in the active tab: start, stop, restart, remove, details, live CPU and memory, logs and shells. View menu."],
    ["Network tools", "Tools menu: a network scanner that finds the computers on a network (192.168.1.0/24, ranges or addresses) and the services they offer, with a button to add an SSH session, and a port scanner that shows which ports of one computer are open, with the greeting each service sends."],
    ["X11 windows", "Tick \"Show windows of programs on the server (X11)\" in a session to run programs with windows on the server and see them here. On Windows the app starts VcXsrv for this when it is installed; Tools, X server shows its state."],
    ["Agent", "An assistant for the SSH server in the active tab (the sparkle button, or View menu). It can read the terminal and files and run commands; anything that changes something is shown to you first. Works with Claude, OpenAI, DeepSeek, Qwen, OpenRouter or a local Ollama model: set one up with the gear in the panel."],
    ["Serial ports", "Open a serial device with its settings, and set DTR, RTS or send a break. File menu."],
    ["Footer", "CPU, memory and disk of this computer or of the active SSH server. Settings."],
    ["Own window", "Move the active tab into a window of its own; its programs keep running. Terminal menu."],
    ["Search and save", "Search the output of a pane, or save all of it as a text file in Downloads. Terminal menu."],
    ["Type into all panes", "What you type goes to every pane of the tab, such as for several servers at once. Terminal menu."],
    ["Import", "Bring in hosts from your ~/.ssh/config file. File menu."],
    ["Settings", "Theme, font, scrollback, renderer, server checks and the footer. File menu."],
  ];

  const keys: [string, string][] = [
    ["Ctrl+Shift+T", "New tab"],
    ["Ctrl+Shift+W", "Close the active pane"],
    ["Ctrl+Tab / Ctrl+Shift+Tab", "Next or previous tab"],
    ["Ctrl+Shift+E", "Split side by side"],
    ["Ctrl+Shift+O", "Split top and bottom"],
    ["Ctrl+Shift+arrows", "Move between panes"],
    ["Ctrl+Shift+F", "Search the output"],
    ["Ctrl+Shift+S", "Save the output to Downloads"],
    ["Ctrl+Shift+B", "Type into all panes"],
    ["Ctrl+Shift+C / V", "Copy and paste"],
  ];
</script>

<div class="modal-backdrop">
  <div
    class="modal wide"
    role="dialog"
    aria-label="Help"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <div class="tabs" role="tablist">
      {#each [["features", "What it can do"], ["shortcuts", "Shortcuts"], ["about", "About"]] as [id, label] (id)}
        <button
          role="tab"
          class="tab"
          class:on={current === id}
          aria-selected={current === id}
          onclick={() => (current = id as typeof current)}>{label}</button
        >
      {/each}
    </div>

    {#if current === "features"}
      <dl class="list">
        {#each features as [name, text] (name)}
          <div><dt>{name}</dt><dd>{text}</dd></div>
        {/each}
      </dl>
    {:else if current === "shortcuts"}
      <dl class="list">
        {#each keys as [key, text] (key)}
          <div><dt><span class="kbd">{key}</span></dt><dd>{text}</dd></div>
        {/each}
      </dl>
    {:else}
      <p>
        SSH Toolbox is a personal terminal and server toolbox for Windows and Ubuntu: terminals, SSH, files, serial
        ports and Docker in one window.
      </p>
      <p><strong>Version {version}</strong></p>
      <p class="muted">Passwords and passphrases are kept in your system keychain, never in the app's files.</p>
    {/if}

    <div class="modal-actions">
      <button class="btn primary" onclick={onclose}>Close</button>
    </div>
  </div>
</div>

<style>
  .modal.wide {
    width: min(660px, 94vw);
  }
  .tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 10px;
  }
  .tab {
    padding: 3px 12px;
    color: var(--text-muted);
    background: transparent;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    font-size: 12px;
    cursor: pointer;
  }
  .tab.on {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }
  .list {
    max-height: 56vh;
    margin: 0 0 8px;
    overflow-y: auto;
  }
  .list div {
    display: flex;
    gap: 14px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  dt {
    flex: none;
    width: 170px;
    font-weight: 600;
  }
  dd {
    margin: 0;
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .muted {
    color: var(--text-muted);
  }
</style>
