// The X server that shows windows of programs run on a server (X11 forwarding).

import { invoke } from "@tauri-apps/api/core";

export interface X11Status {
  /** running: ready for windows. stopped: starts when a session needs it. missing: none can be started. */
  state: "running" | "stopped" | "missing";
  detail: string;
}

export const x11Status = () => invoke<X11Status>("x11_status");
export const x11Start = () => invoke<X11Status>("x11_start");
export const x11Stop = () => invoke<X11Status>("x11_stop");
