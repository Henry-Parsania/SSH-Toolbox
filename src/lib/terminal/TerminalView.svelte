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
  import { AckBatcher, InputSender } from "./flow";
  import { chooseRenderer, webglFailureNotice, type RendererSetting } from "./renderer";

  interface Props {
    spec: SessionSpec;
    platform: PlatformInfo;
    scrollback?: number;
    fontSize?: number;
    fontFamily?: string;
    renderer?: RendererSetting;
    onstatus?: (status: TerminalStatus) => void;
  }

  let {
    spec,
    platform,
    scrollback = 10000,
    fontSize = 14,
    fontFamily = "Cascadia Mono, Consolas, 'DejaVu Sans Mono', 'Liberation Mono', monospace",
    renderer = "auto",
    onstatus,
  }: Props = $props();

  let host: HTMLDivElement;
  let notice = $state("");

  const RESIZE_DEBOUNCE_MS = 50;

  $effect(() => {
    // Settings are read once when the terminal opens. Live changes come with the settings
    // screen (task 1.12).
    return untrack(() => mount());
  });

  function mount(): () => void {
    const encoder = new TextEncoder();
    let disposed = false;
    let sessionId: string | null = null;

    const term = new Terminal({
      scrollback,
      fontSize,
      fontFamily,
      allowProposedApi: true, // Needed for the Unicode 11 addon.
      cursorBlink: true,
      theme: { background: "#1e1e1e", foreground: "#d4d4d4" },
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

    const input = new InputSender((data) => terminalWrite(sessionId!, data));
    const dataSub = term.onData((text) => input.push(encoder.encode(text)));
    const binarySub = term.onBinary((text) =>
      input.push(Uint8Array.from(text, (c) => c.charCodeAt(0))),
    );

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

    term.focus();
    if (import.meta.env.DEV) (window as unknown as { __term?: Terminal }).__term = term;

    return () => {
      disposed = true;
      observer.disconnect();
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

<div class="terminal-wrap">
  <div class="terminal" bind:this={host}></div>
  {#if notice}<p class="notice">{notice}</p>{/if}
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
