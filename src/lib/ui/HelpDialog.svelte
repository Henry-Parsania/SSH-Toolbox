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
    ["Terminals and tabs", "Local shells and WSL, in tabs. The Split button (or Ctrl+Shift+E, and Ctrl+Shift+O for top and bottom in the Terminal menu) divides the window into another pane. Click a pane to work in it: pick one of the open tabs in the bar at the top to show it there, or start a new terminal or saved session in it. Right-click a tab and choose Duplicate to the right or Duplicate below to open another copy of that connection in a new pane (the text isn't copied). Tabs can be dragged into a new order and renamed. When an SSH or serial session has ended, press R to reconnect or C to close it (Esc cancels while connecting). The last layout comes back next time. Terminal menu."],
    ["SSH sessions", "Each session can also run a few commands by itself after connecting, such as cd /srv. Saved in the sessions list, in folders, with password, key or agent login and optional jump hosts. Give a session a colour and its tab and terminal background use it instead of the theme, which helps tell servers apart. A dot shows which servers answer. Right-click a session for connect, monitor, forwarding, key setup, edit and delete. File menu."],
    ["Process monitor", "A button on each SSH session opens btop, htop or top (whichever the server has) in a pop-up window, to watch and manage processes. In the sessions list."],
    ["Key setup", "Make a key (or pick one) and put it on a server so passwords aren't needed, like ssh-copy-id. Tick \"Copy an SSH key to the server after saving\" when creating a session, or use a session's key button or right-click menu."],
    ["Port forwarding", "Local, remote and SOCKS forwards for a session. In the sessions list, on a session's forward button."],
    ["Files", "Browse and copy between this computer and a server (SFTP, FTP or FTPS) by dragging. It can follow the folder the terminal is in. View menu."],
    ["Compare folders", "In the Files panel, the compare button on the This computer side checks that folder against the one on the server, lists what is missing or newer on each side, and copies it either way. Nothing is ever deleted."],
    ["Docker", "Containers, images, volumes and networks on this computer or on the SSH server in the active tab: start, stop, restart, remove, details, live CPU and memory, logs and shells. View menu."],
    ["Network tools", "Tools menu: a network scanner that finds the computers on a network (192.168.1.0/24, ranges or addresses) and the services they offer, with a button to add an SSH session, and a port scanner that shows which ports of one computer are open, with the greeting each service sends."],
    ["Edit server files", "Right-click a text file, Edit in SSH Toolbox: a small editor with Save (Ctrl+S) and Show changes, which compares your text with what was opened. Files over 2 MB or not UTF-8 text are refused. If the file changed on the server meanwhile, it asks before replacing it. You can also double-click a file in the server's file list to open it on this computer in your usual program. When you save it, the app offers to send it back (or does it by itself if you choose). If the file changed on the server meanwhile, it asks before replacing it. The copy is deleted when you stop editing or quit. Programs and scripts ask which program to use. Right-click, Open with, lets you pick the program."],
    ["Trusted servers", "Tools menu: the servers whose keys you accepted (the known_hosts file shared with OpenSSH). Remove one if a server was reinstalled; the next connection asks again. A copy of the file is kept as known_hosts.bak first."],
    ["X11 windows", "Tick \"Show windows of programs on the server (X11)\" in a session to run programs with windows on the server and see them here. On Windows the app starts VcXsrv for this when it is installed; Tools, X server shows its state."],
    ["Snippets", "Saved commands. Terminal menu, Snippets: send one to this pane, every pane of the tab or every open pane. Stored as plain text, so no passwords in them."],
    ["Command palette and quick connect", "Ctrl+Shift+P opens a box where you type to jump to a saved session, an open tab or any menu command. Typing something like pi@10.0.0.5:22 offers to connect to it: the new session form opens filled in, and saving it connects."],
    ["Agent", "An assistant for the SSH server in the active tab (the sparkle button, or View menu). It can read the terminal and files and run commands; anything that changes something is shown to you first. Works with Claude, OpenAI, DeepSeek, Qwen, OpenRouter or a local Ollama model: set one up with the gear in the panel."],
    ["Serial ports", "Open a serial device with its settings, and set DTR, RTS or send a break. File menu."],
    ["Footer", "CPU, memory, network speed, graphics card, disk, load and uptime of this computer or of the active SSH server, with small graphs of the last few minutes. Click the gear at its right end (or right-click the footer) to choose what shows: each reading can be hidden, and its bar, numbers and graph switched on or off. Graphs are off until you turn them on. Your choices are kept. Settings turns the whole footer on or off."],
    ["Long commands", "When a tab that isn't showing has printed output for a while and then goes quiet, a check mark appears on its tab. It goes away when you open the tab."],
    ["Own window", "Move the active tab into a window of its own; its programs keep running. Terminal menu."],
    ["Search and save", "Search the output of a pane, or save all of it as a text file in Downloads. Terminal menu."],
    ["Type into all panes", "What you type goes to every pane of the tab, such as for several servers at once. Terminal menu."],
    ["Back up and restore", "File menu: saves your sessions and folders to one file in Downloads (no passwords, no key contents), and adds the sessions of such a file on another computer without changing the ones already there."],
    ["Reconnect and logs", "A dropped SSH connection is started again by itself (up to 5 tries; Settings turns it off). Settings can also save a plain-text copy of every session in a folder."],
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
    ["Ctrl + / Ctrl - / Ctrl 0", "Make the terminal text bigger, smaller, or back to the normal size"],
    ["Ctrl+Shift+P", "Command palette: jump to a session, tab or command, or type user@host to connect"],
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
