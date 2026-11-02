import { describe, expect, it, vi } from "vitest";
import type { Message, Reply, Risk } from "../backend/agent";
import { AgentChat, type Context, type Deps, type ToolStep } from "./agent.svelte";

const ctx: Context = { sessionName: "web1", savedSessionId: "s1", terminalSession: "t1" };

/** A model that gives the prepared answers one after the other. */
function script(...replies: Partial<Reply>[]) {
  const seen: Message[][] = [];
  return {
    seen,
    complete: vi.fn(async (_id: string, _p: string, _s: string, messages: Message[]) => {
      seen.push(structuredClone(messages));
      const next = replies.shift() ?? { text: "done" };
      return { text: "", tool_calls: [], stop: "done", input_tokens: 10, output_tokens: 5, ...next } as Reply;
    }),
  };
}

function setup(replies: Partial<Reply>[], overrides: Partial<Deps> = {}) {
  const model = script(...replies);
  const deps: Deps = {
    complete: model.complete,
    cancel: vi.fn(async () => {}),
    classify: vi.fn(async (c: string): Promise<Risk> => (c.startsWith("rm -rf /") ? "blocked" : /^(ls|df|cat)\b/.test(c) ? "read_only" : "changes")),
    exec: vi.fn(async () => ({ code: 0, output: "output" })),
    openFiles: vi.fn(async () => "f1"),
    closeFiles: vi.fn(async () => {}),
    readFile: vi.fn(async () => "old\ncontent"),
    writeFile: vi.fn(async () => {}),
    listDir: vi.fn(async () => "a\nb"),
    readTerminal: vi.fn(() => "$ ls\nfile.txt"),
    typeInTerminal: vi.fn(async () => {}),
    redact: vi.fn(async (t: string) => t),
    sleep: vi.fn(async () => {}),
    ...overrides,
  };
  const chat = new AgentChat(deps);
  chat.setProvider("p1");
  return { chat, deps, model };
}

const call = (name: string, input: unknown, id = "c1") => ({ id, name, input });
const tools = (chat: AgentChat) => chat.steps.filter((s): s is ToolStep => s.kind === "tool");
const tick = () => new Promise((r) => setTimeout(r, 0));

describe("the agent loop", () => {
  it("answers without tools", async () => {
    const { chat } = setup([{ text: "Hello." }]);
    await chat.send("hi", () => ctx);
    expect(chat.steps.map((s) => s.kind)).toEqual(["user", "assistant"]);
    expect(chat.usage).toEqual({ input: 10, output: 5 });
    expect(chat.running).toBe(false);
  });

  it("asks before running anything and sends the result back", async () => {
    const { chat, deps, model } = setup([{ stop: "tools", tool_calls: [call("run_command", { command: "ls -la" })] }, { text: "Two files." }]);
    const done = chat.send("what is here", () => ctx);
    await tick();
    await tick();
    const step = tools(chat)[0];
    expect(step.state).toBe("waiting");
    expect(deps.exec).not.toHaveBeenCalled();
    chat.answer(step.id, true);
    await done;
    expect(deps.exec).toHaveBeenCalledWith("s1", "ls -la", true);
    const last = model.seen[1].at(-1);
    expect(last).toMatchObject({ role: "tool_results", results: [{ call_id: "c1", is_error: false }] });
    expect(chat.steps.at(-1)).toMatchObject({ kind: "assistant", text: "Two files." });
  });

  it("runs commands that only look without asking when the user allowed it, but never changes", async () => {
    const { chat, deps } = setup([
      { stop: "tools", tool_calls: [call("run_command", { command: "df -h" }, "a"), call("run_command", { command: "systemctl restart x" }, "b")] },
      { text: "ok" },
    ]);
    chat.setAutoRead(true);
    const done = chat.send("check", () => ctx);
    await tick();
    await tick();
    await tick();
    expect(deps.exec).toHaveBeenCalledTimes(1);
    expect(deps.exec).toHaveBeenCalledWith("s1", "df -h", false);
    const waiting = tools(chat).find((s) => s.state === "waiting");
    expect(waiting?.detail).toBe("systemctl restart x");
    chat.answer(waiting!.id, false);
    await done;
    expect(deps.exec).toHaveBeenCalledTimes(1);
  });

  it("tells the model when the user said no, and refuses blocked commands without asking", async () => {
    const { chat, deps, model } = setup([
      { stop: "tools", tool_calls: [call("run_command", { command: "rm -rf /" }, "a"), call("run_command", { command: "reboot" }, "b")] },
      { text: "understood" },
    ]);
    const done = chat.send("go", () => ctx);
    await tick();
    await tick();
    const waiting = tools(chat).find((s) => s.state === "waiting")!;
    expect(waiting.detail).toBe("reboot");
    chat.answer(waiting.id, false);
    await done;
    expect(deps.exec).not.toHaveBeenCalled();
    const results = (model.seen[1].at(-1) as Extract<Message, { role: "tool_results" }>).results;
    expect(results.map((r) => r.is_error)).toEqual([true, true]);
    expect(results[0].content).toContain("too dangerous");
    expect(results[1].content).toContain("did not approve");
  });

  it("shows what a file write would change and writes only after approval", async () => {
    const { chat, deps } = setup([{ stop: "tools", tool_calls: [call("write_file", { path: "/etc/x.conf", content: "old\nnew" })] }, { text: "saved" }]);
    const done = chat.send("edit", () => ctx);
    await tick();
    await tick();
    await tick();
    const step = tools(chat)[0];
    expect(step.state).toBe("waiting");
    expect(step.preview).toContain("- content");
    expect(step.preview).toContain("+ new");
    expect(deps.writeFile).not.toHaveBeenCalled();
    chat.answer(step.id, true);
    await done;
    expect(deps.writeFile).toHaveBeenCalledWith("f1", "/etc/x.conf", "old\nnew", true);
  });

  it("works on nothing when the tab is not an SSH session, and reads the terminal with secrets removed", async () => {
    const { chat, deps } = setup(
      [{ stop: "tools", tool_calls: [call("run_command", { command: "ls" }, "a"), call("read_terminal", { lines: 20, wait_seconds: 2 }, "b")] }, { text: "ok" }],
      { redact: vi.fn(async () => "[clean]") },
    );
    await chat.send("look", () => ({ sessionName: "local", savedSessionId: null, terminalSession: "t1" }));
    expect(deps.exec).not.toHaveBeenCalled();
    expect(deps.sleep).toHaveBeenCalledWith(2000);
    expect(deps.redact).toHaveBeenCalledWith("$ ls\nfile.txt");
    expect(tools(chat).find((s) => s.name === "read_terminal")?.result).toBe("[clean]");
  });

  it("asks first before reading a file that may hold secrets", async () => {
    const readFile = vi.fn(async (_f: string, _p: string, approved: boolean) => {
      if (!approved) throw { code: "permission_denied", message: "secrets" };
      return "key material";
    });
    const { chat } = setup([{ stop: "tools", tool_calls: [call("read_file", { path: "/root/.ssh/id_ed25519" })] }, { text: "ok" }], { readFile });
    const done = chat.send("read it", () => ctx);
    await tick();
    await tick();
    await tick();
    const step = tools(chat)[0];
    expect(step.state).toBe("waiting");
    chat.answer(step.id, true);
    await done;
    expect(readFile).toHaveBeenLastCalledWith("f1", "/root/.ssh/id_ed25519", true);
  });

  it("stops: the request is cancelled, waiting steps are refused and the history stays valid", async () => {
    const { chat, deps, model } = setup([{ stop: "tools", tool_calls: [call("run_command", { command: "reboot" })] }, { text: "never" }]);
    const done = chat.send("go", () => ctx);
    await tick();
    await tick();
    chat.stop();
    await done;
    expect(deps.exec).not.toHaveBeenCalled();
    expect(chat.running).toBe(false);
    expect(chat.steps.at(-1)).toMatchObject({ kind: "note", text: "Stopped." });
    // A second question works, and the earlier tool call has its result.
    await chat.send("again", () => ctx);
    const roles = model.seen.at(-1)!.map((m) => m.role);
    expect(roles).toEqual(["user", "assistant", "tool_results", "user"]);
  });

  it("stops after too many rounds and reports model errors", async () => {
    const endless = Array.from({ length: 20 }, () => ({ stop: "tools" as const, tool_calls: [call("list_dir", { path: "/" })] }));
    const { chat } = setup(endless);
    await chat.send("loop", () => ctx);
    expect(chat.steps.at(-1)).toMatchObject({ kind: "note" });
    expect((chat.steps.at(-1) as { text: string }).text).toContain("Stopped after");

    const failing = setup([], { complete: vi.fn(async () => Promise.reject({ message: "The provider didn't accept the API key." })) });
    await failing.chat.send("hi", () => ctx);
    expect(failing.chat.steps.at(-1)).toMatchObject({ kind: "note", text: "The provider didn't accept the API key." });
  });

  it("can run commands in the visible terminal and read the result and exit code back from the screen", async () => {
    const screen: string[] = ["$ "];
    const typeInTerminal = vi.fn(async (_s: string, text: string) => {
      const id = /toolbox-(\w+) \$\?/.exec(text)![1];
      screen.push(`$ ${text.replace(/^\x15/, "").replace(/\r$/, "")}`, "hello from the server", `[toolbox-${id} 3]`, "$ ");
    });
    const { chat, deps } = setup([{ stop: "tools", tool_calls: [call("run_command", { command: "ls /srv" })] }, { text: "ok" }], {
      typeInTerminal,
      readTerminal: vi.fn(() => screen.join("\n")),
    });
    chat.setUseTerminal(true);
    chat.setAutoRead(true);
    await chat.send("list", () => ctx);
    expect(deps.exec).not.toHaveBeenCalled();
    const typed = typeInTerminal.mock.calls[0][1];
    expect(typed.startsWith("\x15ls /srv; echo ")).toBe(true);
    expect(typed.endsWith("\r")).toBe(true);
    const step = tools(chat)[0];
    expect(step.state).toBe("done");
    expect(step.result).toBe("Exit code 3\nhello from the server");
  });

  it("needs a provider", async () => {
    const { chat, deps } = setup([]);
    chat.setProvider("");
    await chat.send("hi", () => ctx);
    expect(deps.complete).not.toHaveBeenCalled();
    expect(chat.steps[0].kind).toBe("note");
  });
});
