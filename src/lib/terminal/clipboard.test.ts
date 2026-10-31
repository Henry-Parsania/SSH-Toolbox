import { describe, expect, it } from "vitest";
import { checkPaste, isCopyKey, isPasteKey } from "./clipboard";

describe("checkPaste", () => {
  it("does not ask for a single line", () => {
    const r = checkPaste("ls -la");
    expect(r.needsConfirm).toBe(false);
    expect(r.lineCount).toBe(1);
  });

  it("does not ask for empty text", () => {
    expect(checkPaste("").needsConfirm).toBe(false);
  });

  it.each([["a\nb"], ["a\r\nb"], ["a\rb"]])("asks for line breaks: %j", (text) => {
    const r = checkPaste(text);
    expect(r.needsConfirm).toBe(true);
    expect(r.lineCount).toBe(2);
    expect(r.preview).toEqual(["a", "b"]);
  });

  it("asks when one command ends with a line break, since Enter would run it", () => {
    expect(checkPaste("rm -rf x\n").needsConfirm).toBe(true);
  });

  it("shows only the first few lines and says there are more", () => {
    const r = checkPaste(Array.from({ length: 12 }, (_, i) => `line${i}`).join("\n"));
    expect(r.preview).toEqual(["line0", "line1", "line2", "line3", "line4"]);
    expect(r.truncated).toBe(true);
    expect(r.lineCount).toBe(12);
  });

  it("cuts very long lines in the preview only", () => {
    const r = checkPaste("x".repeat(500) + "\nb");
    expect(r.preview[0].length).toBe(121);
    expect(r.preview[0].endsWith("…")).toBe(true);
  });
});

describe("shortcut keys", () => {
  const key = (code: string, extra: Partial<KeyboardEvent> = {}) => ({
    type: "keydown",
    ctrlKey: true,
    shiftKey: true,
    altKey: false,
    metaKey: false,
    code,
    ...extra,
  });

  it("recognises Ctrl+Shift+C and Ctrl+Shift+V only", () => {
    expect(isCopyKey(key("KeyC"))).toBe(true);
    expect(isPasteKey(key("KeyV"))).toBe(true);
    expect(isCopyKey(key("KeyV"))).toBe(false);
    expect(isPasteKey(key("KeyC"))).toBe(false);
  });

  it("leaves plain Ctrl+C alone, since it interrupts the program", () => {
    expect(isCopyKey(key("KeyC", { shiftKey: false }))).toBe(false);
    expect(isPasteKey(key("KeyV", { shiftKey: false }))).toBe(false);
  });

  it("ignores key releases and Alt/Meta combinations", () => {
    expect(isCopyKey(key("KeyC", { type: "keyup" }))).toBe(false);
    expect(isPasteKey(key("KeyV", { altKey: true }))).toBe(false);
    expect(isCopyKey(key("KeyC", { metaKey: true }))).toBe(false);
  });
});
