// Docker: connecting here or over a saved SSH session, and the container list.

import { Channel, invoke } from "@tauri-apps/api/core";
import type { TerminalStatus } from "./index";

export type DockerTarget = { type: "local" } | { type: "ssh"; saved_session_id: string };

export interface ContainerInfo {
  id: string;
  name: string;
  image: string;
  /** running, exited, paused, restarting, created, dead or removing. */
  state: string;
  /** Docker's own wording, such as "Up 2 hours". */
  status: string;
  /** The Compose project the container belongs to, if any. */
  project: string | null;
  service: string | null;
  ports: string[];
}

export interface ContainerDetails {
  name: string;
  image: string;
  state: string;
  tty: boolean;
  env: string[];
  ports: string[];
  mounts: string[];
  networks: string[];
  /** The complete inspect output. Show as text only. */
  raw: string;
}

export type ContainerAction = "start" | "stop" | "restart" | "remove";

/** Connects to Docker. Questions about the SSH login arrive through onStatus. */
export async function dockerConnect(
  target: DockerTarget,
  operationId: string,
  onStatus: (status: TerminalStatus) => void,
): Promise<string> {
  const channel = new Channel<TerminalStatus>();
  channel.onmessage = onStatus;
  const opened = await invoke<{ docker_id: string }>("docker_connect", {
    target,
    operationId,
    onStatus: channel,
  });
  return opened.docker_id;
}

export function dockerClose(dockerId: string): Promise<void> {
  return invoke<void>("docker_close", { dockerId });
}

export function dockerList(dockerId: string): Promise<ContainerInfo[]> {
  return invoke<ContainerInfo[]>("docker_list", { dockerId });
}

export function dockerAction(dockerId: string, containerId: string, action: ContainerAction): Promise<void> {
  return invoke<void>("docker_action", { dockerId, containerId, action });
}

export function dockerInspect(dockerId: string, containerId: string): Promise<ContainerDetails> {
  return invoke<ContainerDetails>("docker_inspect", { dockerId, containerId });
}
