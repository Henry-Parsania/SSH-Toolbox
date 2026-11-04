// Docker commands the app types into a terminal tab for you (tasks 7.14 to 7.17). Running them
// in a terminal shows the output as it comes, and anything that needs a password (a registry
// login) is typed there, never into this app. Every value is checked first, because it ends up
// inside single quotes in a shell the app doesn't control.

// Paths and names: nothing that could end a quote or start another command.
const SAFE = /^[^'"`$;&|<>(){}\u0000-\u001f\u007f\u2018-\u201f]+$/;
const IMAGE = /^[A-Za-z0-9][A-Za-z0-9._\-/:@]*$/;
const CONTAINER = /^[A-Za-z0-9][A-Za-z0-9_.-]*$/;
const REGISTRY = /^[A-Za-z0-9][A-Za-z0-9._\-/:]*$/;

export type Checked = { command: string } | { error: string };

const q = (text: string) => `'${text}'`;
const path = (text: string) => text.trim() !== "" && SAFE.test(text);

export function buildCommand(tag: string, context: string, dockerfile: string): Checked {
  if (!IMAGE.test(tag)) return { error: "The image name can use letters, digits and . _ - / : only, such as myapp:1.0." };
  if (!path(context)) return { error: "Choose the folder that holds the Dockerfile (no quotes or special characters)." };
  if (dockerfile.trim() && !path(dockerfile)) return { error: "That Dockerfile path can't be used." };
  const file = dockerfile.trim() ? ` -f ${q(dockerfile.trim())}` : "";
  return { command: `docker build -t ${q(tag)}${file} ${q(context.trim())}` };
}

export function tagCommand(source: string, target: string): Checked {
  if (!IMAGE.test(source) || !IMAGE.test(target)) {
    return { error: "Image names can use letters, digits and . _ - / : only, such as registry.example.com/team/app:1.0." };
  }
  return { command: `docker tag ${q(source)} ${q(target)}` };
}

export function pushCommand(name: string): Checked {
  if (!IMAGE.test(name)) return { error: "Image names can use letters, digits and . _ - / : only." };
  return { command: `docker push ${q(name)}` };
}

/** Docker asks for the user name and password itself, in the terminal. */
export function loginCommand(registry: string): Checked {
  if (!registry.trim()) return { command: "docker login" };
  if (!REGISTRY.test(registry.trim())) return { error: "That registry address can't be used." };
  return { command: `docker login ${q(registry.trim())}` };
}

export function attachCommand(container: string): Checked {
  if (!CONTAINER.test(container)) return { error: "That container name can't be used." };
  // Without --sig-proxy=false, Ctrl+C in the tab would stop the container.
  return { command: `docker attach --sig-proxy=false ${q(container)}` };
}

export function networkCommand(action: "connect" | "disconnect", network: string, container: string): Checked {
  if (!CONTAINER.test(network)) return { error: "That network name can't be used." };
  if (!CONTAINER.test(container)) return { error: "That container name can't be used." };
  return { command: `docker network ${action} ${q(network)} ${q(container)}` };
}

export function copyOutCommand(container: string, containerPath: string, destination: string): Checked {
  if (!CONTAINER.test(container)) return { error: "That container name can't be used." };
  if (!path(containerPath) || !path(destination)) return { error: "Both paths are needed, without quotes or special characters." };
  return { command: `docker cp ${q(`${container}:${containerPath.trim()}`)} ${q(destination.trim())}` };
}

export function copyInCommand(source: string, container: string, containerPath: string): Checked {
  if (!CONTAINER.test(container)) return { error: "That container name can't be used." };
  if (!path(source) || !path(containerPath)) return { error: "Both paths are needed, without quotes or special characters." };
  return { command: `docker cp ${q(source.trim())} ${q(`${container}:${containerPath.trim()}`)}` };
}
