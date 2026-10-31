// Copy and paste rules (INTERFACES.md section 12).

const PREVIEW_LINES = 5;
const PREVIEW_LINE_WIDTH = 120;

export interface PasteCheck {
  /** True when the text has a line break, so pasting could run commands straight away. */
  needsConfirm: boolean;
  lineCount: number;
  /** The first few lines, each cut short, for the confirmation box. */
  preview: string[];
  /** True when there are more lines than the preview shows. */
  truncated: boolean;
}

/** Splits on any kind of line break (\r\n, \n or \r). */
function lines(text: string): string[] {
  return text.split(/\r\n|\n|\r/);
}

/**
 * Removes control characters from text about to be pasted, other than tab and line breaks. An
 * escape character in the clipboard could otherwise end the terminal's protected paste early and
 * make the rest run as typed commands.
 */
export function sanitizePaste(text: string): string {
  // C0 controls except tab, line feed and carriage return; DEL; and the C1 controls.
  return text.replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/g, "");
}

export function checkPaste(text: string): PasteCheck {
  const all = lines(text);
  // A single trailing line break doesn't make it multi-line: it's one line plus Enter. Still
  // ask, because that Enter runs the command.
  const needsConfirm = all.length > 1;
  const shown = all.slice(0, PREVIEW_LINES).map((l) =>
    l.length > PREVIEW_LINE_WIDTH ? l.slice(0, PREVIEW_LINE_WIDTH) + "…" : l,
  );
  return {
    needsConfirm,
    lineCount: all.length,
    preview: shown,
    truncated: all.length > PREVIEW_LINES,
  };
}

/** True for the copy shortcut, Ctrl+Shift+C. */
export function isCopyKey(e: Pick<KeyboardEvent, "type" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey" | "code">): boolean {
  return e.type === "keydown" && e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey && e.code === "KeyC";
}

/** True for the paste shortcut, Ctrl+Shift+V. */
export function isPasteKey(e: Pick<KeyboardEvent, "type" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey" | "code">): boolean {
  return e.type === "keydown" && e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey && e.code === "KeyV";
}
