<script lang="ts">
  import { untrack } from "svelte";
  import { keepAlive } from "../layout/detach";
  import { registerTerminalReader } from "./access";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { Unicode11Addon } from "@xterm/addon-unicode11";
  import { SearchAddon } from "@xterm/addon-search";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { WebglAddon } from "@xterm/addon-webgl";
  import "@xterm/xterm/css/xterm.css";
  import {
    openLink,
    promptRespond,
    startupReady,
    terminalAck,
    terminalAttach,
    terminalClose,
    terminalSaveOutput,
    terminalOpen,
    terminalResize,
    terminalWrite,
    type PlatformInfo,
    type Prompt,
    type PromptResponse,
    type SessionSpec,
    type TerminalStatus,
  } from "../backend";
  import Icon from "../ui/Icon.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import { checkPaste, isCopyKey, isPasteKey, sanitizePaste, type PasteCheck } from "./clipboard";
  import { AckBatcher, InputSender } from "./flow";
  import { settings } from "../settings.svelte";
  import { broadcastInput, registerReceiver } from "./broadcast";
  import { chooseRenderer, webglFailureNotice } from "./renderer";
  import { parseCwd } from "./cwd";
  import { terminalTheme } from "./themes";

  interface Props {
    spec: SessionSpec;
    platform: PlatformInfo;
    onstatus?: (status: TerminalStatus) => void;
    /** Give the keyboard to this terminal whenever this becomes true. */
    focused?: boolean;
    /** Opens the search box each time this number goes up. */
    searchSignal?: number;
    /** Saves the output to a file each time this number goes up. */
    saveSignal?: number;
    /** Shows this open session (moved here from another window) instead of starting one. */
    attachId?: string;
    /** Identify this pane, so typing can be copied to the others in the tab. */
    tabId?: string;
    paneId?: string;
    broadcasting?: boolean;
    /** Called with the folder the shell says it is in (OSC 7). */
    oncwd?: (path: string) => void;
    /** Told the backend session ID once it is open, and null when it is closed. */
    onsession?: (id: string | null) => void;
  }

  let {
    spec,
    platform,
    onstatus,
    focused = true,
    searchSignal = 0,
    saveSignal = 0,
    attachId = "",
    tabId = "",
    paneId = "",
    broadcasting = false,
    oncwd,
    onsession,
  }: Props = $props();

  let searching = $state(false);
  let query = $state("");
  let noMatch = $state(false);
  let searchInput: HTMLInputElement | undefined = $state();
  let findNext: (backwards: boolean) => void = () => {};
  let clearSearch: () => void = () => {};
  let saveHook: () => void = () => {};

  $effect(() => {
    if (saveSignal > 0) untrack(() => saveHook());
  });

  $effect(() => {
    if (searchSignal > 0) {
      searching = true;
      queueMicrotask(() => searchInput?.select());
    }
  });

  function closeSearch() {
    searching = false;
    noMatch = false;
    clearSearch();
    refocus();
  }

  let focusTerminal: () => void = () => {};
  let applyAppearance: () => void = () => {};
  // Font, size, theme and scrollback follow the settings as they change. The renderer is
  // chosen when the terminal opens.
  $effect(() => {
    void [settings.font_family, settings.font_size, settings.theme, settings.scrollback];
    applyAppearance();
  });

  $effect(() => {
    if (focused) focusTerminal();
  });

  let host: HTMLDivElement;
  let notice = $state("");
  // A question from the backend (host key, password). Shown over the terminal until answered.
  let question = $state<{ id: string; prompt: Prompt } | null>(null);

  function answerQuestion(response: PromptResponse) {
    if (!question) return;
    void promptRespond(question.id, response).catch(() => {});
    question = null;
    refocus();
  }
  let background = $state("#1e1e1e");
  let pending = $state<{ text: string; check: PasteCheck } | null>(null);
  // Set by mount(), so the confirmation box can reach the terminal.
  let pasteConfirmed: (text: string) => void = () => {};
  let refocus: () => void = () => {};

  function confirmPaste() {
    if (!pending) return;
    pasteConfirmed(pending.text);
    pending = null;
    refocus();
  }

  function cancelPaste() {
    pending = null;
    refocus();
  }

  const RESIZE_DEBOUNCE_MS = 50;

  // The first output of the first terminal marks "usable" for the startup time (Q3).
  let startupReported = false;
  function reportStartup() {
    if (startupReported) return;
    startupReported = true;
    void startupReady().catch(() => {});
  }

  /** The chosen font first, then the usual monospace fonts on each system. */
  function fontStack(family: string): string {
    // The plain word "monospace" is the default setting. It gives a small, dated font, so the
    // good fonts of each system are used instead.
    if (family.trim().toLowerCase() === "monospace") {
      return `Cascadia Mono, 'SF Mono', Consolas, 'DejaVu Sans Mono', 'Liberation Mono', monospace`;
    }
    return `${family}, Cascadia Mono, Consolas, 'DejaVu Sans Mono', 'Liberation Mono', monospace`;
  }

  $effect(() => {
    return untrack(() => mount());
  });

  function mount(): () => void {
    const encoder = new TextEncoder();
    let disposed = false;
    let sessionId: string | null = null;

    const renderer = settings.renderer;
    const term = new Terminal({
      scrollback: settings.scrollback,
      fontSize: settings.font_size,
      fontFamily: fontStack(settings.font_family),
      theme: terminalTheme(settings.theme),
      allowProposedApi: true, // Needed for the Unicode 11 addon.
      cursorBlink: true,
      // Links the program in the terminal marks up itself (OSC 8) follow the same rule as
      // plain addresses: Ctrl+click only, and through the backend, which allows only http and
      // https (SEC9). Without this, xterm would open them itself on a plain click.
      linkHandler: {
        activate: (event, uri) => {
          if (event.ctrlKey || event.metaKey) void openLink(uri).catch(() => {});
        },
        allowNonHttpProtocols: false,
      },
      ...(platform.os === "windows" && platform.windows_build !== null
        ? { windowsPty: { backend: "conpty" as const, buildNumber: platform.windows_build } }
        : {}),
    });
    const fit = new FitAddon();
    // The shell may report its folder. The value is untrusted text: it is cleaned first, and
    // only changes which folder the file panel shows.
    term.parser.registerOscHandler(7, (data) => {
      const path = parseCwd(data);
      if (path) oncwd?.(path);
      return true;
    });
    term.loadAddon(fit);
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";
    const search = new SearchAddon();
    term.loadAddon(search);
    // Links open only on Ctrl+click (or Cmd+click), and only through the backend, which allows
    // nothing but http and https (SEC9).
    term.loadAddon(
      new WebLinksAddon((event, uri) => {
        if (event.ctrlKey || event.metaKey) void openLink(uri).catch(() => {});
      }),
    );
    findNext = (backwards) => {
      if (!query) return;
      const found = backwards ? search.findPrevious(query) : search.findNext(query);
      noMatch = !found;
    };
    clearSearch = () => search.clearDecorations();
    term.open(host);

    if (chooseRenderer(renderer, platform.os) === "webgl") {
      try {
        const webgl = new WebglAddon();
        // If the graphics context is lost, drop back to the DOM renderer.
        webgl.onContextLoss(() => {
          webgl.dispose();
          notice = webglFailureNotice(renderer) ?? "";
        });
        term.loadAddon(webgl);
      } catch {
        notice = webglFailureNotice(renderer) ?? "";
      }
    }

    // Output: bytes go straight to xterm, and each chunk is acknowledged once xterm has
    // processed it. That acknowledgement is what lets the backend keep sending.
    const acks = new AckBatcher((bytes) => {
      if (sessionId) void terminalAck(sessionId, bytes).catch(() => {});
    });
    // Acknowledgements for output that arrives before the session ID is known.
    let earlyAcks = 0;

    pasteConfirmed = (text) => term.paste(text);
    refocus = () => term.focus();
    applyAppearance = () => {
      term.options.fontSize = settings.font_size;
      term.options.fontFamily = fontStack(settings.font_family);
      term.options.theme = terminalTheme(settings.theme);
      term.options.scrollback = settings.scrollback;
      background = terminalTheme(settings.theme).background;
      fitIfVisible();
    };
    background = terminalTheme(settings.theme).background;
    focusTerminal = () => term.focus();

    const input = new InputSender((data) => terminalWrite(sessionId!, data));
    const unregister = registerReceiver(tabId, paneId, (data) => input.push(data));
    const dataSub = term.onData((text) => {
      const bytes = encoder.encode(text);
      input.push(bytes);
      if (broadcasting) broadcastInput(tabId, paneId, bytes);
    });
    const binarySub = term.onBinary((text) => {
      const bytes = Uint8Array.from(text, (c) => c.charCodeAt(0));
      input.push(bytes);
      if (broadcasting) broadcastInput(tabId, paneId, bytes);
    });

    // Copy and paste (T2). Every paste goes through here, whatever started it, so the
    // multi-line warning can't be skipped.
    function requestPaste(raw: string) {
      const text = sanitizePaste(raw);
      if (!text) return;
      const check = checkPaste(text);
      if (check.needsConfirm) pending = { text, check };
      else term.paste(text);
    }
    const onPasteEvent = (e: ClipboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      requestPaste(e.clipboardData?.getData("text/plain") ?? "");
    };
    // Capture phase, so it runs before xterm's own paste handler.
    host.addEventListener("paste", onPasteEvent, true);

    // What the agent reads: the last lines of the screen and its scrollback, as plain text.
    let unregisterReader = () => {};
    function readText(maxLines: number): string {
      const buffer = term.buffer.active;
      const lines: string[] = [];
      for (let i = Math.max(0, buffer.length - maxLines - 50); i < buffer.length; i++) {
        lines.push(buffer.getLine(i)?.translateToString(true) ?? "");
      }
      while (lines.length && lines[lines.length - 1] === "") lines.pop();
      return lines.slice(-maxLines).join("\n");
    }

    // Save the whole scrollback as a text file in Downloads (T9).
    saveHook = saveOutput;
    function saveOutput() {
      const buffer = term.buffer.active;
      const lines: string[] = [];
      for (let i = 0; i < buffer.length; i++) lines.push(buffer.getLine(i)?.translateToString(true) ?? "");
      while (lines.length && lines[lines.length - 1] === "") lines.pop();
      void terminalSaveOutput(lines.join("\n") + "\n").then(
        (path) => (notice = `Saved to ${path}`),
        (err: { message?: string }) => (notice = err.message ?? "Couldn't save the output."),
      );
      setTimeout(() => (notice = ""), 6000);
    }

    term.attachCustomKeyEventHandler((e) => {
      if (isCopyKey(e)) {
        e.preventDefault();
        const selection = term.getSelection();
        if (selection) void navigator.clipboard.writeText(selection).catch(() => {});
        return false;
      }
      if (e.type === "keydown" && e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey && e.code === "KeyS") {
        e.preventDefault();
        saveOutput();
        return false;
      }
      if (isPasteKey(e)) {
        e.preventDefault();
        void navigator.clipboard.readText().then(requestPaste, () => {});
        return false;
      }
      return true;
    });

    function handleStatus(status: TerminalStatus) {
      if (status.type === "prompt") {
        question = { id: status.prompt_id, prompt: status.prompt };
      } else if (status.type === "connected" || status.type === "failed" || status.type === "exited") {
        question = null;
      }
      if (status.type === "exited") {
        input.stop();
        const how = status.code === null ? "" : ` with code ${status.code}`;
        term.write(`\r\n\x1b[2m[Process exited${how}]\x1b[0m\r\n`);
      } else if (status.type === "failed") {
        input.stop();
        term.write(`\r\n\x1b[31m[${status.error.message}]\x1b[0m\r\n`);
      }
      onstatus?.(status);
    }

    // Fit only when visible: a hidden pane has no size to fit to.
    function fitIfVisible() {
      if (host.clientWidth > 0 && host.clientHeight > 0) fit.fit();
    }

    let resizeTimer: ReturnType<typeof setTimeout> | null = null;
    const observer = new ResizeObserver(() => {
      if (resizeTimer !== null) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(fitIfVisible, RESIZE_DEBOUNCE_MS);
    });
    observer.observe(host);

    let openedCols = 0;
    let openedRows = 0;
    const resizeSub = term.onResize(({ cols, rows }) => {
      if (sessionId) void terminalResize(sessionId, cols, rows).catch(() => {});
    });

    fitIfVisible();
    openedCols = term.cols;
    openedRows = term.rows;

    const onChunk = (chunk: Uint8Array) => {
      reportStartup();
      term.write(chunk, () => {
        if (sessionId) acks.processed(chunk.length);
        else earlyAcks += chunk.length;
      });
    };
    const starting = attachId
      ? terminalAttach(attachId, onChunk, handleStatus)
      : terminalOpen(spec, openedCols, openedRows, onChunk, handleStatus);
    starting.then(
      (id) => {
        if (disposed) {
          void terminalClose(id).catch(() => {});
          return;
        }
        sessionId = id;
        unregisterReader = registerTerminalReader(id, readText);
        onsession?.(id);
        input.enable();
        if (earlyAcks > 0) acks.processed(earlyAcks);
        earlyAcks = 0;
        // The window may have been resized while the session was starting.
        if (term.cols !== openedCols || term.rows !== openedRows) {
          void terminalResize(id, term.cols, term.rows).catch(() => {});
        }
      },
      (err: { message?: string }) => {
        handleStatus({
          type: "failed",
          error: { code: "internal", message: err.message ?? "Couldn't open the terminal." },
        });
      },
    );

    if (focused) term.focus();
    if (import.meta.env.DEV) (window as unknown as { __term?: Terminal }).__term = term;

    return () => {
      disposed = true;
      observer.disconnect();
      host.removeEventListener("paste", onPasteEvent, true);
      if (resizeTimer !== null) clearTimeout(resizeTimer);
      acks.dispose();
      input.stop();
      unregister();
      dataSub.dispose();
      binarySub.dispose();
      resizeSub.dispose();
      // A session handed to another window keeps running.
      if (sessionId && keepAlive.has(sessionId)) keepAlive.delete(sessionId);
      else if (sessionId) void terminalClose(sessionId).catch(() => {});
      unregisterReader();
      onsession?.(null);
      term.dispose();
    };
  }
</script>

<div class="terminal-wrap" style:background>
  <div class="terminal" bind:this={host}></div>
  {#if notice}<p class="notice">{notice}</p>{/if}
  {#if question}
    <PromptDialog prompt={question.prompt} onanswer={answerQuestion} />
  {/if}
  {#if searching}
    <div class="search">
      <span class="s-icon"><Icon name="search" size={14} /></span>
      <input
        bind:this={searchInput}
        bind:value={query}
        placeholder="Search"
        oninput={() => findNext(false)}
        onkeydown={(e) => {
          if (e.key === "Enter") findNext(e.shiftKey);
          else if (e.key === "Escape") closeSearch();
          e.stopPropagation();
        }}
        class:nomatch={noMatch}
      />
      <button class="icon-btn small" aria-label="Previous match" title="Previous (Shift+Enter)" onclick={() => findNext(true)}><Icon name="up" size={14} /></button>
      <button class="icon-btn small" aria-label="Next match" title="Next (Enter)" onclick={() => findNext(false)}><Icon name="down" size={14} /></button>
      <button class="icon-btn small" aria-label="Close search" title="Close (Esc)" onclick={closeSearch}><Icon name="close" size={14} /></button>
    </div>
  {/if}
  {#if pending}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="confirm"
      role="dialog"
      aria-label="Paste multiple lines?"
      tabindex="-1"
      onkeydown={(e) => {
        if (e.key === "Escape") cancelPaste();
      }}
    >
      <p class="title">Paste {pending.check.lineCount} lines?</p>
      <pre>{pending.check.preview.join("\n")}{pending.check.truncated ? "\n…" : ""}</pre>
      <div class="buttons">
        <button class="btn small" onclick={cancelPaste}>Cancel</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn small primary" autofocus onclick={confirmPaste}>Paste</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .terminal-wrap {
    position: relative;
    width: 100%;
    height: 100%;
  }
  .terminal {
    width: 100%;
    height: 100%;
    padding: 6px 4px 4px 10px;
  }
  .notice {
    position: absolute;
    right: 12px;
    bottom: 12px;
    z-index: 4;
    margin: 0;
    padding: 6px 12px;
    color: var(--warning);
    background: var(--bg-2);
    border: 1px solid #f0b45a55;
    border-radius: var(--radius);
    font-size: 12px;
    box-shadow: var(--shadow-small);
  }

  /* Search box, in the top right corner. */
  .search {
    position: absolute;
    top: 10px;
    right: 18px;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 4px 4px 10px;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-small);
    animation: pop-in 0.12s var(--ease);
  }
  .s-icon {
    display: grid;
    margin-right: 6px;
    color: var(--text-faint);
  }
  .search input {
    width: 15em;
    padding: 3px 6px;
    background: transparent;
    border-color: transparent;
    box-shadow: none;
  }
  .search input:focus {
    box-shadow: none;
    border-color: transparent;
  }
  .search input.nomatch {
    color: var(--danger);
  }

  /* The warning before pasting several lines. */
  .confirm {
    position: absolute;
    top: 50%;
    left: 50%;
    z-index: 5;
    width: min(520px, 86%);
    transform: translate(-50%, -50%);
    padding: 16px 18px;
    color: var(--text);
    background: var(--bg-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow);
    animation: pop-in 0.16s var(--ease);
  }
  .confirm .title {
    margin: 0 0 10px;
    font-size: 14px;
    font-weight: 600;
  }
  .confirm pre {
    max-height: 11em;
    margin: 0 0 14px;
    padding: 9px 11px;
    overflow: auto;
    color: var(--text);
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    font: 12px/1.5 var(--mono);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
