// Connects the agent loop to the real backend.

import { terminalWrite } from "../backend";
import {
  agentCancel,
  agentClassify,
  agentComplete,
  agentExec,
  agentReadFile,
  agentRedact,
  agentSettingsGet,
  agentWriteFile,
} from "../backend/agent";
import { filesClose, filesList, filesOpenSftp } from "../backend/files";
import { readTerminalText } from "../terminal/access";
import { formatSize } from "../files/paths";
import type { Deps } from "./agent.svelte";

export const realDeps: Deps = {
  complete: agentComplete,
  cancel: agentCancel,
  classify: agentClassify,
  settings: agentSettingsGet,
  exec: agentExec,
  async openFiles(savedSessionId) {
    // The session is already connected, so this opens SFTP on the same connection and asks nothing.
    const opened = await filesOpenSftp(crypto.randomUUID(), savedSessionId, () => {});
    return opened.id;
  },
  closeFiles: filesClose,
  readFile: agentReadFile,
  writeFile: agentWriteFile,
  async listDir(filesId, path) {
    const entries = await filesList(filesId, path);
    const shown = entries.slice(0, 300).map((e) => {
      const kind = e.kind === "dir" ? "/" : e.kind === "symlink" ? "@" : "";
      const size = e.kind === "file" && e.size !== null ? `  ${formatSize(e.size)}` : "";
      return `${e.name}${kind}${size}`;
    });
    if (entries.length > shown.length) shown.push(`… and ${entries.length - shown.length} more`);
    return shown.join("\n") || "(empty)";
  },
  readTerminal: readTerminalText,
  typeInTerminal: (session, text) => terminalWrite(session, new TextEncoder().encode(text)),
  redact: agentRedact,
  sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
};
