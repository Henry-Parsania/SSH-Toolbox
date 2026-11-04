// Small helpers for showing sizes and times in the footer.

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function bytes(n: number): string {
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${UNITS[unit]}`;
}

/** A speed, such as "1.2 MB/s". */
export function rate(bytesPerSecond: number): string {
  return `${bytes(bytesPerSecond)}/s`;
}

export function percent(used: number, total: number): number {
  return total > 0 ? Math.min(100, Math.max(0, (used / total) * 100)) : 0;
}

export function uptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}
