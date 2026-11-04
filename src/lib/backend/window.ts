// The app window's own controls (the title bar is drawn by the page).
import { getCurrentWindow } from "@tauri-apps/api/window";

export function appWindow() {
  return getCurrentWindow();
}
