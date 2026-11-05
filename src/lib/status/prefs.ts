// What the footer shows (the owner's choices, kept between runs). Each reading can be shown or
// hidden, and the ones that make sense can be a gauge bar, numbers and a small graph.

export type MetricId = "cpu" | "ram" | "net" | "gpu" | "disk" | "load" | "uptime";
export type Part = "bar" | "numbers" | "graph";

export interface MetricPref {
  show: boolean;
  bar: boolean;
  numbers: boolean;
  graph: boolean;
}

export type FooterPrefs = Record<MetricId, MetricPref>;

/** The readings in the order they appear, with the parts each one can have. */
export const METRICS: { id: MetricId; label: string; parts: Part[] }[] = [
  { id: "cpu", label: "Processor", parts: ["bar", "numbers", "graph"] },
  { id: "ram", label: "Memory", parts: ["bar", "numbers", "graph"] },
  { id: "net", label: "Network", parts: ["numbers", "graph"] },
  { id: "gpu", label: "Graphics card", parts: ["bar", "numbers", "graph"] },
  { id: "disk", label: "Disk", parts: ["bar", "numbers"] },
  { id: "load", label: "Load average", parts: ["numbers"] },
  { id: "uptime", label: "Uptime", parts: ["numbers"] },
];

/** Everything is shown as before; every graph is off. */
export function defaultPrefs(): FooterPrefs {
  const out = {} as FooterPrefs;
  for (const m of METRICS) out[m.id] = { show: true, bar: true, numbers: true, graph: false };
  return out;
}

/**
 * Reads saved choices. Anything missing or of the wrong kind falls back to the default, so an
 * old, edited or damaged value can never break the footer.
 */
export function parsePrefs(text: string | null): FooterPrefs {
  const prefs = defaultPrefs();
  if (!text) return prefs;
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return prefs;
  }
  if (typeof raw !== "object" || raw === null) return prefs;
  for (const m of METRICS) {
    const saved = (raw as Record<string, unknown>)[m.id];
    if (typeof saved !== "object" || saved === null) continue;
    for (const key of ["show", "bar", "numbers", "graph"] as const) {
      const value = (saved as Record<string, unknown>)[key];
      if (typeof value === "boolean") prefs[m.id][key] = value;
    }
  }
  return prefs;
}
