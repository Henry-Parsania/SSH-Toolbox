// Which xterm renderer to use (INTERFACES.md section 11).

export type RendererSetting = "auto" | "dom" | "webgl";
export type Renderer = "dom" | "webgl";

/**
 * `auto` is WebGL on Windows and the DOM renderer elsewhere. WebGL can be created on Linux and
 * still be slow, and the app can't tell, which is why the setting exists.
 */
export function chooseRenderer(setting: RendererSetting, os: "windows" | "linux" | "macos"): Renderer {
  if (setting === "dom" || setting === "webgl") return setting;
  return os === "windows" ? "webgl" : "dom";
}

/** The message to show when WebGL was asked for but didn't work, or none for `auto`. */
export function webglFailureNotice(setting: RendererSetting): string | null {
  return setting === "webgl"
    ? "WebGL couldn't start, so the basic renderer is being used. You can change this in settings."
    : null;
}
