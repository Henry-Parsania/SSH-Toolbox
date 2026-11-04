// The `docker compose` commands for a Compose project (task 7.13). They are typed into a terminal
// tab, so the user sees the output as it comes and the commands run in their own environment.
// The project's folder and files come from labels Compose put on its containers.

import type { ContainerInfo } from "../backend/docker";

export type ComposeAction = "up" | "down" | "pull" | "restart" | "logs";

export const COMPOSE_WORDS: Record<ComposeAction, string> = {
  up: "up -d",
  down: "down",
  pull: "pull",
  restart: "restart",
  logs: "logs -f --tail 200",
};

// Values go inside single quotes in the command, in a shell the app doesn't control (PowerShell,
// bash, cmd...). Anything that could break out of the quotes, or run something, is refused.
const SAFE = /^[^'"`$;&|<>(){}\u0000-\u001f\u007f\u2018-\u201f]+$/;

export interface ComposeProject {
  name: string;
  dir: string;
  files: string[];
}

/** The project's folder and files, from the labels of its containers. Null if they are missing. */
export function projectOf(name: string, containers: ContainerInfo[]): ComposeProject | null {
  const withLabels = containers.find((c) => c.project === name && c.project_dir && c.config_files);
  if (!withLabels) return null;
  return {
    name,
    dir: withLabels.project_dir!,
    files: withLabels.config_files!.split(",").map((f) => f.trim()).filter(Boolean),
  };
}

/** The command for an action, or null when the project's paths can't be used safely. */
export function composeCommand(project: ComposeProject, action: ComposeAction): string | null {
  const parts = [project.name, project.dir, ...project.files];
  if (project.files.length === 0 || !parts.every((p) => SAFE.test(p))) return null;
  if (!/^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(project.name)) return null;
  const files = project.files.map((f) => `-f '${f}'`).join(" ");
  return `docker compose --project-directory '${project.dir}' -p '${project.name}' ${files} ${COMPOSE_WORDS[action]}`;
}
