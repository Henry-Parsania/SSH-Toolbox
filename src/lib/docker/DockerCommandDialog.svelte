<script lang="ts">
  // A small form for a Docker command that runs in a terminal tab: build an image, tag or push
  // it, log in to a registry, or copy files to or from a container. Nothing here handles a
  // password: a registry login asks for it in the terminal.

  import { untrack } from "svelte";
  import {
    buildCommand,
    copyInCommand,
    copyOutCommand,
    loginCommand,
    networkCommand,
    pushCommand,
    tagCommand,
    type Checked,
  } from "./commands";

  export type CommandKind = "build" | "tag" | "push" | "login" | "cp-out" | "cp-in" | "net-connect" | "net-disconnect";

  interface Props {
    kind: CommandKind;
    /** A container name, for the copy commands. */
    container?: string;
    /** An image name, for tag and push. */
    image?: string;
    /** Where the paths are: "this computer" or the server's name. */
    where: string;
    onrun: (command: string, title: string) => void;
    onclose: () => void;
  }

  let { kind, container = "", image = "", where, onrun, onclose }: Props = $props();

  // The form starts from what it was opened with and doesn't follow later changes.
  let a = $state(untrack(() => (kind === "tag" || kind === "push" ? image : "")));
  let b = $state("");
  let c = $state("");
  let error = $state("");

  const titles: Record<CommandKind, string> = {
    build: "Build an image",
    tag: "Tag an image",
    push: "Push an image",
    login: "Log in to a registry",
    "cp-out": "Copy a file out of a container",
    "cp-in": "Copy a file into a container",
    "net-connect": "Connect a container to a network",
    "net-disconnect": "Disconnect a container from a network",
  };

  function submit() {
    let result: Checked;
    let title: string;
    switch (kind) {
      case "build":
        result = buildCommand(a.trim(), b, c);
        title = `build ${a.trim()}`;
        break;
      case "tag":
        result = tagCommand(a.trim(), b.trim());
        title = `tag ${b.trim()}`;
        break;
      case "push":
        result = pushCommand(a.trim());
        title = `push ${a.trim()}`;
        break;
      case "login":
        result = loginCommand(a);
        title = "docker login";
        break;
      case "net-connect":
      case "net-disconnect":
        result = networkCommand(kind === "net-connect" ? "connect" : "disconnect", a.trim(), container);
        title = `${kind === "net-connect" ? "connect" : "disconnect"} ${container}`;
        break;
      case "cp-out":
        result = copyOutCommand(container, a, b);
        title = `copy from ${container}`;
        break;
      default:
        result = copyInCommand(a, container, b);
        title = `copy to ${container}`;
    }
    if ("error" in result) {
      error = result.error;
      return;
    }
    onclose();
    onrun(result.command, title);
  }
</script>

<div class="modal-backdrop">
  <div
    class="modal"
    role="dialog"
    aria-label={titles[kind]}
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      else if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) submit();
      e.stopPropagation();
    }}
  >
    <h2>{titles[kind]}</h2>
    <p class="lead">
      This types the command into a new terminal tab, so you can watch it run. Paths are on {where}.
    </p>

    {#if kind === "build"}
      <label class="field">Name for the image <input type="text" bind:value={a} placeholder="myapp:1.0" spellcheck="false" /></label>
      <label class="field">Folder with the Dockerfile <input type="text" bind:value={b} placeholder="/srv/app" spellcheck="false" /></label>
      <label class="field">Dockerfile (optional, if it isn't called Dockerfile) <input type="text" bind:value={c} spellcheck="false" /></label>
    {:else if kind === "tag"}
      <label class="field">Image <input type="text" bind:value={a} spellcheck="false" /></label>
      <label class="field">New name <input type="text" bind:value={b} placeholder="registry.example.com/team/app:1.0" spellcheck="false" /></label>
    {:else if kind === "push"}
      <label class="field">Image to push <input type="text" bind:value={a} spellcheck="false" /></label>
      <small>You need to be logged in to the registry first.</small>
    {:else if kind === "login"}
      <label class="field">
        Registry (empty = Docker Hub)
        <input type="text" bind:value={a} placeholder="registry.example.com" spellcheck="false" />
        <small>Docker asks for your user name and password in the terminal. They are never typed into this app.</small>
      </label>
    {:else if kind === "net-connect" || kind === "net-disconnect"}
      <label class="field">Network name <input type="text" bind:value={a} placeholder="backend" spellcheck="false" /></label>
      <small>The container {container} {kind === "net-connect" ? "joins" : "leaves"} this network. The Networks tab lists the names.</small>
    {:else if kind === "cp-out"}
      <label class="field">Path in {container} <input type="text" bind:value={a} placeholder="/etc/nginx/nginx.conf" spellcheck="false" /></label>
      <label class="field">Copy it to (on {where}) <input type="text" bind:value={b} placeholder="./nginx.conf" spellcheck="false" /></label>
    {:else}
      <label class="field">File on {where} <input type="text" bind:value={a} spellcheck="false" /></label>
      <label class="field">Put it at (in {container}) <input type="text" bind:value={b} placeholder="/tmp/" spellcheck="false" /></label>
    {/if}

    {#if error}<p class="message error">{error}</p>{/if}
    <div class="modal-actions">
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" onclick={submit}>Run in a tab</button>
    </div>
  </div>
</div>
