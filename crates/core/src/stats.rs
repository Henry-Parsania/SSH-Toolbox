//! The load of a computer, for the footer: processor, memory, disk, and how long it has been
//! on. A server is asked with one short command on its existing connection (Linux servers only),
//! and this computer is read directly.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stats {
    /// Share of the processor in use, 0 to 100. Not known until two readings exist.
    pub cpu_percent: Option<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    /// The one, five and fifteen minute load, as the system writes it.
    pub load: Option<String>,
    pub uptime_secs: Option<u64>,
}

/// Processor time counters, kept between readings to work out the share in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

/// Run on the server. Everything it prints is read by `parse_linux`, nothing else is used.
pub const REMOTE_COMMAND: &str = "head -n1 /proc/stat; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo; df -Pk / | tail -n1; cat /proc/loadavg; cut -d. -f1 /proc/uptime";

fn number(word: Option<&str>) -> Option<u64> {
    word?.parse().ok()
}

/// Reads what `REMOTE_COMMAND` printed. Returns `None` for anything that isn't Linux-like. The
/// text comes from the server, so every part is checked and a bad part is just left out.
pub fn parse_linux(output: &str, previous: Option<CpuTimes>) -> Option<(Stats, CpuTimes)> {
    let mut times = None;
    let (mut mem_total, mut mem_free) = (None, None);
    let (mut disk_total, mut disk_used) = (None, None);
    let mut load = None;
    let mut uptime = None;

    for line in output.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.first().copied() {
            Some("cpu") => {
                let counters: Vec<u64> = words[1..].iter().filter_map(|w| w.parse().ok()).collect();
                if counters.len() >= 4 {
                    let total: u64 = counters.iter().take(8).sum();
                    let idle = counters[3] + counters.get(4).copied().unwrap_or(0);
                    times = Some(CpuTimes {
                        busy: total.saturating_sub(idle),
                        total,
                    });
                }
            }
            Some("MemTotal:") => {
                mem_total = number(words.get(1).copied()).map(|k| k.saturating_mul(1024))
            }
            Some("MemAvailable:") => {
                mem_free = number(words.get(1).copied()).map(|k| k.saturating_mul(1024))
            }
            Some(first) if first.starts_with('/') || first.contains(':') || first == "tmpfs" => {
                // A line of `df -Pk`: name, size, used, available, percent, mount point.
                if words.len() >= 6 {
                    disk_total = number(words.get(1).copied()).map(|k| k.saturating_mul(1024));
                    disk_used = number(words.get(2).copied()).map(|k| k.saturating_mul(1024));
                }
            }
            Some(first) if first.contains('.') && words.len() >= 4 => {
                // `/proc/loadavg`: three load figures, then running/total and the last process.
                if words[..3].iter().all(|w| w.parse::<f32>().is_ok()) {
                    load = Some(words[..3].join(" "));
                }
            }
            Some(only) if words.len() == 1 && only.chars().all(|c| c.is_ascii_digit()) => {
                uptime = only.parse().ok();
            }
            _ => {}
        }
    }

    let now = times?;
    let cpu_percent = previous.and_then(|before| {
        let total = now.total.checked_sub(before.total)?;
        if total == 0 {
            return None;
        }
        let busy = now.busy.saturating_sub(before.busy);
        Some((busy as f32 / total as f32 * 100.0).clamp(0.0, 100.0))
    });
    let mem_total = mem_total?;
    Some((
        Stats {
            cpu_percent,
            mem_used: mem_total.saturating_sub(mem_free.unwrap_or(0)),
            mem_total,
            disk_used: disk_used.unwrap_or(0),
            disk_total: disk_total.unwrap_or(0),
            load,
            uptime_secs: uptime,
        },
        now,
    ))
}

/// Readings of this computer. Keep one around: the processor share needs the one before it.
pub struct LocalStats {
    system: sysinfo::System,
}

impl Default for LocalStats {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalStats {
    pub fn new() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_cpu_usage();
        Self { system }
    }

    /// `home` picks the disk to report: the one that holds it.
    pub fn read(&mut self, home: Option<&Path>) -> Stats {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let disk = home
            .and_then(|home| {
                disks
                    .list()
                    .iter()
                    .filter(|d| home.starts_with(d.mount_point()))
                    .max_by_key(|d| d.mount_point().as_os_str().len())
            })
            .or_else(|| disks.list().iter().max_by_key(|d| d.total_space()));
        let (disk_total, disk_used) = disk
            .map(|d| {
                (
                    d.total_space(),
                    d.total_space().saturating_sub(d.available_space()),
                )
            })
            .unwrap_or((0, 0));
        Stats {
            cpu_percent: Some(self.system.global_cpu_usage()),
            mem_used: self.system.used_memory(),
            mem_total: self.system.total_memory(),
            disk_used,
            disk_total,
            load: None,
            uptime_secs: Some(sysinfo::System::uptime()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_A: &str = "cpu  1000 0 500 8000 100 0 0 0 0 0\nMemTotal:       16000000 kB\nMemAvailable:    4000000 kB\n/dev/sda1 100000000 40000000 55000000 43% /\n0.52 0.40 0.35 2/512 12345\n86400\n";
    const SAMPLE_B: &str = "cpu  1300 0 700 8400 100 0 0 0 0 0\nMemTotal:       16000000 kB\nMemAvailable:    4000000 kB\n/dev/sda1 100000000 40000000 55000000 43% /\n0.52 0.40 0.35 2/512 12345\n86401\n";

    #[test]
    fn reads_memory_disk_load_and_uptime() {
        let (stats, _) = parse_linux(SAMPLE_A, None).unwrap();
        assert_eq!(stats.mem_total, 16_000_000 * 1024);
        assert_eq!(stats.mem_used, 12_000_000 * 1024);
        assert_eq!(stats.disk_total, 100_000_000 * 1024);
        assert_eq!(stats.disk_used, 40_000_000 * 1024);
        assert_eq!(stats.load.as_deref(), Some("0.52 0.40 0.35"));
        assert_eq!(stats.uptime_secs, Some(86_400));
        assert_eq!(stats.cpu_percent, None, "one reading can't give a share");
    }

    #[test]
    fn the_processor_share_comes_from_two_readings() {
        let (_, first) = parse_linux(SAMPLE_A, None).unwrap();
        let (stats, _) = parse_linux(SAMPLE_B, Some(first)).unwrap();
        // 500 busy of 900 elapsed.
        let cpu = stats.cpu_percent.unwrap();
        assert!((cpu - 55.6).abs() < 0.1, "{cpu}");
    }

    #[test]
    fn output_from_something_else_is_ignored() {
        assert!(parse_linux("", None).is_none());
        assert!(parse_linux("head: cannot open '/proc/stat'\n", None).is_none());
        assert!(parse_linux("cpu  a b c d\n", None).is_none());
    }

    #[test]
    fn a_server_that_lies_cannot_break_the_numbers() {
        let weird = "cpu  1 1 1 1\nMemTotal: 99999999999999999999 kB\n";
        assert!(parse_linux(weird, None).is_none());
        let backwards = parse_linux(
            SAMPLE_B,
            Some(CpuTimes {
                busy: 0,
                total: u64::MAX,
            }),
        )
        .unwrap()
        .0;
        assert_eq!(backwards.cpu_percent, None);
    }

    #[test]
    fn this_computer_can_be_read() {
        let mut local = LocalStats::new();
        let stats = local.read(None);
        assert!(stats.mem_total > 0);
        assert!(stats.mem_used <= stats.mem_total);
    }
}
