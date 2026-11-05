import { defaultPrefs, parsePrefs, type FooterPrefs, type MetricId, type Part } from "./prefs";

const KEY = "footer-prefs";

function load(): FooterPrefs {
  try {
    return parsePrefs(localStorage.getItem(KEY));
  } catch {
    return defaultPrefs();
  }
}

/** The footer's choices. Changing one saves it at once. */
export const footerPrefs = $state<{ value: FooterPrefs }>({ value: load() });

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(footerPrefs.value)));
  } catch {
    /* The choices just aren't kept if storage is unavailable. */
  }
}

export function setPart(id: MetricId, part: Part | "show", on: boolean) {
  footerPrefs.value[id][part] = on;
  save();
}

export function resetFooter() {
  footerPrefs.value = defaultPrefs();
  save();
}
