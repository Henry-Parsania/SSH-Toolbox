import { describe, expect, it } from "vitest";
import { snippetBytes } from "./send";

const text = (b: Uint8Array) => new TextDecoder().decode(b);

describe("snippetBytes", () => {
  it("types the text and presses Enter when it should run", () => {
    expect(text(snippetBytes({ text: "ls -la", run: true }))).toBe("ls -la\r");
    expect(text(snippetBytes({ text: "ls -la", run: false }))).toBe("ls -la");
  });

  it("separates lines with Enter and ignores blank lines at the end", () => {
    expect(text(snippetBytes({ text: "cd /srv\r\ndocker ps\n\n", run: true }))).toBe("cd /srv\rdocker ps\r");
  });
});
