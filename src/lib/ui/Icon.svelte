<script lang="ts">
  // A small set of line icons, drawn on a 24 by 24 grid, in the colour of the surrounding text.
  export type IconName =
    | "menu"
    | "plus"
    | "chevron-down"
    | "chevron-right"
    | "close"
    | "settings"
    | "search"
    | "terminal"
    | "server"
    | "key"
    | "route"
    | "import"
    | "folder"
    | "folder-plus"
    | "play"
    | "edit"
    | "trash"
    | "sidebar"
    | "split-h"
    | "split-v"
    | "broadcast"
    | "refresh"
    | "check"
    | "up"
    | "down"
    | "serial"
    | "ftp"
    | "home"
    | "file"
    | "files"
    | "box"
    | "stop"
    | "info"
    | "log";

  interface Props {
    name: IconName;
    size?: number;
  }

  let { name, size = 16 }: Props = $props();

  const paths: Record<IconName, string> = {
    menu: "M4 7h16M4 12h16M4 17h16",
    plus: "M12 5v14M5 12h14",
    "chevron-down": "M6 9l6 6 6-6",
    "chevron-right": "M9 6l6 6-6 6",
    close: "M6 6l12 12M18 6L6 18",
    settings:
      "M12 15.5a3.5 3.5 0 100-7 3.5 3.5 0 000 7zM19.4 13.5a1.5 1.5 0 00.3 1.7l.1.1a1.8 1.8 0 11-2.5 2.5l-.1-.1a1.5 1.5 0 00-1.7-.3 1.5 1.5 0 00-.9 1.4v.2a1.8 1.8 0 11-3.6 0v-.1a1.5 1.5 0 00-1-1.4 1.5 1.5 0 00-1.7.3l-.1.1a1.8 1.8 0 11-2.5-2.5l.1-.1a1.5 1.5 0 00.3-1.7 1.5 1.5 0 00-1.4-.9h-.2a1.8 1.8 0 110-3.6h.1a1.5 1.5 0 001.4-1 1.5 1.5 0 00-.3-1.7l-.1-.1a1.8 1.8 0 112.5-2.5l.1.1a1.5 1.5 0 001.7.3h.1a1.5 1.5 0 00.9-1.4v-.2a1.8 1.8 0 113.6 0v.1a1.5 1.5 0 00.9 1.4 1.5 1.5 0 001.7-.3l.1-.1a1.8 1.8 0 112.5 2.5l-.1.1a1.5 1.5 0 00-.3 1.7v.1a1.5 1.5 0 001.4.9h.2a1.8 1.8 0 010 3.6h-.1a1.5 1.5 0 00-1.4.9z",
    search: "M11 18a7 7 0 100-14 7 7 0 000 14zM21 21l-4.3-4.3",
    terminal: "M4 5h16a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V6a1 1 0 011-1zM7 10l3 2.5L7 15M12.5 15H17",
    server:
      "M5 4h14a1 1 0 011 1v5a1 1 0 01-1 1H5a1 1 0 01-1-1V5a1 1 0 011-1zM5 13h14a1 1 0 011 1v5a1 1 0 01-1 1H5a1 1 0 01-1-1v-5a1 1 0 011-1zM8 7.5h.01M8 16.5h.01",
    key: "M14.5 9.5a4.5 4.5 0 11-4.2 6.1L4 21.5V19l2-.5V16.5l2-.5 2.2-2.2A4.5 4.5 0 0114.5 9.5zM16 8.5h.01",
    route: "M6 19a2 2 0 100-4 2 2 0 000 4zM18 9a2 2 0 100-4 2 2 0 000 4zM8 17h6.5a3.5 3.5 0 000-7h-5a3.5 3.5 0 010-7H16",
    import: "M12 4v11m0 0l-4-4m4 4l4-4M5 19h14",
    folder: "M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z",
    "folder-plus": "M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7zM12 11v5M9.5 13.5h5",
    play: "M8 5.5v13l11-6.5-11-6.5z",
    edit: "M4 20h4L19 9a2.1 2.1 0 00-3-3L5 17v3zM14 8l3 3",
    trash: "M5 7h14M10 7V5h4v2M7 7l1 12h8l1-12M10 11v5M14 11v5",
    sidebar: "M4 5h16a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V6a1 1 0 011-1zM9 5v14",
    "split-h": "M4 5h16a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V6a1 1 0 011-1zM12 5v14",
    "split-v": "M4 5h16a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V6a1 1 0 011-1zM3 12h18",
    broadcast: "M12 13a1.5 1.5 0 100-3 1.5 1.5 0 000 3zM8.5 8.5a5 5 0 000 7M15.5 8.5a5 5 0 010 7M5.5 5.5a9 9 0 000 13M18.5 5.5a9 9 0 010 13",
    refresh: "M20 11a8 8 0 00-14.5-4M4 4v4h4M4 13a8 8 0 0014.5 4M20 20v-4h-4",
    check: "M5 12.5l4.5 4.5L19 7.5",
    up: "M12 19V5m0 0l-5 5m5-5l5 5",
    down: "M12 5v14m0 0l-5-5m5 5l5-5",
    serial: "M5 8h14v8H5zM8 8V5M12 8V5M16 8V5M8 16v3M16 16v3",
    ftp: "M4 6h7l2 2h7v10H4zM12 11v5m0 0l-2-2m2 2l2-2",
    home: "M4 11l8-7 8 7M6 10v9a1 1 0 001 1h3v-6h4v6h3a1 1 0 001-1v-9",
    file: "M7 3h7l5 5v12a1 1 0 01-1 1H7a1 1 0 01-1-1V4a1 1 0 011-1zM14 3v5h5",
    files: "M4 6a2 2 0 012-2h3l2 2h5a2 2 0 012 2v1M3 11h15a1 1 0 011 1l-1 6a2 2 0 01-2 2H5a2 2 0 01-2-2v-7zM21 11v-0",
    box: "M12 3l8 4v10l-8 4-8-4V7l8-4zM4 7l8 4 8-4M12 11v10",
    stop: "M7 7h10v10H7z",
    info: "M12 21a9 9 0 100-18 9 9 0 000 18zM12 11v6M12 7.5h.01",
    log: "M7 3h7l5 5v12a1 1 0 01-1 1H7a1 1 0 01-1-1V4a1 1 0 011-1zM14 3v5h5M9 13h6M9 17h6",
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path d={paths[name]} />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
