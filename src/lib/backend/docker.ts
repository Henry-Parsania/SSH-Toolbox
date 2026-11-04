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
  /** Where the Compose project was started from, and its compose files (comma separated). */
  project_dir?: string | null;
  config_files?: string | null;
  ports: string[];
  /** Seconds since 1970. */
  created: number;
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

export type ContainerAction = "start" | "stop" | "restart" | "pause" | "unpause" | "kill" | "remove";

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

export interface ContainerStats {
  id: string;
  /** 100 is one full processor core. */
  cpu_percent: number;
  memory_used: number;
  memory_limit: number;
}

export function dockerStats(dockerId: string): Promise<ContainerStats[]> {
  return invoke<ContainerStats[]>("docker_stats", { dockerId });
}

export type ResourceKind = "images" | "volumes" | "networks";

export interface DockerResource {
  /** What is passed back to remove it. */
  id: string;
  name: string;
  detail: string;
  /** Docker's own networks can't be removed. */
  protected: boolean;
  /** Seconds since 1970, for images. */
  created: number | null;
  /** Bytes it takes, if known. */
  size: number | null;
  /** The containers that use it. Empty: nothing does. */
  used_by: string[];
  /** The Compose project it belongs to. */
  project: string | null;
}

export function dockerResources(dockerId: string, kind: ResourceKind): Promise<DockerResource[]> {
  return invoke<DockerResource[]>("docker_resources", { dockerId, kind });
}

export function dockerRemoveResource(dockerId: string, kind: ResourceKind, id: string): Promise<void> {
  return invoke<void>("docker_remove_resource", { dockerId, kind, id });
}

export function dockerRename(dockerId: string, containerId: string, name: string): Promise<void> {
  return invoke<void>("docker_rename", { dockerId, containerId, name });
}

export interface ResourceDetails {
  lines: string[];
  /** The complete answer. Show as text only. */
  raw: string;
}

export function dockerResourceDetails(dockerId: string, kind: ResourceKind, id: string): Promise<ResourceDetails> {
  return invoke<ResourceDetails>("docker_resource_details", { dockerId, kind, id });
}

export type PruneKind = "containers" | "images" | "volumes" | "networks";

export function dockerPrune(dockerId: string, kind: PruneKind): Promise<{ removed: number; bytes: number }> {
  return invoke("docker_prune", { dockerId, kind });
}

export interface PullProgress {
  done: number;
  /** Grows while the download starts, as Docker reports the size of each part. */
  total: number;
  status: string;
}

/** Downloads an image. Resolves when it is done; onProgress says how it is going. */
export function dockerPull(dockerId: string, reference: string, onProgress: (p: PullProgress) => void): Promise<void> {
  const channel = new Channel<PullProgress>();
  channel.onmessage = onProgress;
  return invoke<void>("docker_pull", { dockerId, reference, onProgress: channel });
}

export interface RunSpec {
  image: string;
  name: string;
  ports: { host: number; container: number; udp: boolean }[];
  /** Publish the ports on 127.0.0.1 only. */
  localhost_only: boolean;
  volumes: { source: string; target: string; read_only: boolean }[];
  /** NAME=value lines. */
  env: string[];
  restart: "no" | "always" | "unless-stopped" | "on-failure";
}

/** Starts a new container from an image (downloading it first if needed). Returns its ID. */
export function dockerRun(dockerId: string, spec: RunSpec): Promise<string> {
  return invoke<string>("docker_run", { dockerId, spec });
}

/** Makes an empty volume or network. */
export function dockerCreateResource(dockerId: string, kind: "volumes" | "networks", name: string): Promise<void> {
  return invoke<void>("docker_create_resource", { dockerId, kind, name });
}
