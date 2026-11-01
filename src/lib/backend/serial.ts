// Serial ports: the list, and the control lines of an open serial session.

import { invoke } from "@tauri-apps/api/core";

export interface PortInfo {
  /** What is passed to open it: COM3 or /dev/ttyUSB0. */
  name: string;
  /** A short description, such as the USB device's name. May be empty. */
  label: string;
}

export function serialListPorts(): Promise<PortInfo[]> {
  return invoke<PortInfo[]>("serial_list_ports");
}

/** Sets the DTR and RTS lines. A line left out stays as it is. */
export function serialSetControl(
  sessionId: string,
  lines: { dtr?: boolean; rts?: boolean },
): Promise<void> {
  return invoke<void>("serial_set_control", { sessionId, dtr: lines.dtr ?? null, rts: lines.rts ?? null });
}

export function serialSendBreak(sessionId: string): Promise<void> {
  return invoke<void>("serial_send_break", { sessionId });
}
