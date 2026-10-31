<script lang="ts">
  import { untrack } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { Unicode11Addon } from "@xterm/addon-unicode11";
  import { WebglAddon } from "@xterm/addon-webgl";
  import "@xterm/xterm/css/xterm.css";
  import {
    terminalAck,
    terminalClose,
    terminalOpen,
    terminalResize,
    terminalWrite,
    type PlatformInfo,
    type SessionSpec,
    type TerminalStatus,
  } from "../backend";
  import { checkPaste, isCopyKey, isPasteKey, type PasteCheck } from "./clipboard";
  import { AckBatcher, InputSender } from "./flow";
  import { settings } from "../settings.svelte";
  import { chooseRenderer, webglFailureNotice } from "./renderer";
  import { terminalTheme } from "./themes";

  interface Props {
    spec: SessionSpec;
    platform: PlatformInfo;
    onstatus?: (status: TerminalStatus) => void;
    /** Give the keyboard to this terminal whenever this becomes true. */
    focused?: boolean;
  }

  let {
    spec,
    platform,
    onstatus,
    focused = true,
  }: Props = $props();

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

  /** The chosen font first, then the usual monospace fonts on each system. */
  function fontStack(family: string): string {
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
      ...(platform.os === "windows" && platform.windows_build !== null
        ? { windowsPty: { backend: "conpty" as const, buildNumber: platform.windows_build } }
        : {}),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";
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
    const dataSub = term.onData((text) => input.push(encoder.encode(text)));
    const binarySub = term.onBinary((text) =>
      input.push(Uint8Array.from(text, (c) => c.charCodeAt(0))),
    );

    // Copy and paste (T2). Every paste goes through here, whatever started it, so the
    // multi-line warning can't be skipped.
    function requestPaste(text: string) {
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

    term.attachCustomKeyEventHandler((e) => {
      if (isCopyKey(e)) {
        e.preventDefault();
        const selection = term.getSelection();
        if (selection) void navigator.clipboard.writeText(selection).catch(() => {});
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

    terminalOpen(
      spec,
      openedCols,
      openedRows,
      (chunk) => {
        term.write(chunk, () => {
          if (sessionId) acks.processed(chunk.length);
          else earlyAcks += chunk.length;
        });
      },
      handleStatus,
    ).then(
      (id) => {
        if (disposed) {
          void terminalClose(id).catch(() => {});
          return;
        }
        sessionId = id;
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
      dataSub.dispose();
      binarySub.dispose();
      resizeSub.dispose();
      if (sessionId) void terminalClose(sessionId).catch(() => {});
      term.dispose();
    };
  }
</script>

<div class="terminal-wrap" style:background>
  <div class="terminal" bind:this={host}></div>
  {#if notice}<p class="notice">{notice}</p>{/if}
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
        <button onclick={cancelPaste}>Cancel</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button autofocus onclick={confirmPaste}>Paste</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .terminal-wrap {
    position: relative;
    width: 100%;
    height: 100%;
    background: #1e1e1e;
  }
  .terminal {
    width: 100%;
    height: 100%;
    padding: 4px;
    box-sizing: border-box;
  }
  .confirm {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    max-width: 80%;
    padding: 12px 16px;
    background: #252526;
    color: #d4d4d4;
    border: 1px solid #555;
    border-radius: 6px;
    font: 13px system-ui, sans-serif;
    box-shadow: 0 4px 16px #0008;
  }
  .confirm .title {
    margin: 0 0 8px;
    font-weight: 600;
  }
  .confirm pre {
    margin: 0 0 12px;
    padding: 6px 8px;
    background: #1e1e1e;
    overflow: auto;
    max-height: 10em;
    font: 12px Consolas, monospace;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .notice {
    position: absolute;
    right: 8px;
    bottom: 8px;
    margin: 0;
    padding: 4px 8px;
    background: #5a4b00;
    color: #fff;
    font: 12px system-ui, sans-serif;
    border-radius: 4px;
  }
</style>
