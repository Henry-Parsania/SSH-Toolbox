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
    /// Bytes received and sent since the computer started, over all its network connections
    /// except the loopback. The screen works out the speed from two readings.
    pub net_rx_bytes: Option<u64>,
    pub net_tx_bytes: Option<u64>,
    /// Share of the graphics card in use (the busiest one), and its memory. Only when the
    /// computer has an NVIDIA card and `nvidia-smi`.
    pub gpu_percent: Option<f32>,
    pub gpu_mem_used: Option<u64>,
    pub gpu_mem_total: Option<u64>,
}

/// Processor time counters, kept between readings to work out the share in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

/// Run on the server. Everything it prints is read by `parse_linux`, nothing else is used.
pub const REMOTE_COMMAND: &str = "head -n1 /proc/stat; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo; df -Pk / | tail -n1; cat /proc/loadavg; cut -d. -f1 /proc/uptime; echo '#net'; cat /proc/net/dev 2>/dev/null; echo '#gpu'; nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null";

/// Adds up the received and sent bytes of every interface except the loopback, from the text of
/// `/proc/net/dev`.
pub fn parse_net_dev(text: &str) -> Option<(u64, u64)> {
    let (mut rx, mut tx, mut any) = (0u64, 0u64, false);
    for line in text.lines() {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        if name.trim() == "lo" {
            continue;
        }
        let fields: Vec<u64> = rest
            .split_whitespace()
            .map_while(|w| w.parse().ok())
            .collect();
        // Receive bytes are the first column and transmit bytes the ninth.
        if fields.len() >= 9 {
            rx = rx.saturating_add(fields[0]);
            tx = tx.saturating_add(fields[8]);
            any = true;
        }
    }
    any.then_some((rx, tx))
}

/// What `nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits`
/// prints: one line per card, such as `37, 1024, 8192`. The busiest card's share is used, and the
/// memory of all cards is added up (in bytes).
pub fn parse_gpu(text: &str) -> Option<(f32, u64, u64)> {
    let (mut busiest, mut used, mut total, mut any) = (0f32, 0u64, 0u64, false);
    for line in text.lines() {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() != 3 {
            continue;
        }
        let (Ok(share), Ok(u), Ok(t)) = (
            parts[0].parse::<f32>(),
            parts[1].parse::<u64>(),
            parts[2].parse::<u64>(),
        ) else {
            continue;
        };
        busiest = busiest.max(share.clamp(0.0, 100.0));
        used = used.saturating_add(u.saturating_mul(1024 * 1024));
        total = total.saturating_add(t.saturating_mul(1024 * 1024));
        any = true;
    }
    any.then_some((busiest, used, total))
}

fn number(word: Option<&str>) -> Option<u64> {
    word?.parse().ok()
}

/// Reads what `REMOTE_COMMAND` printed. Returns `None` for anything that isn't Linux-like. The
/// text comes from the server, so every part is checked and a bad part is just left out.
pub fn parse_linux(output: &str, previous: Option<CpuTimes>) -> Option<(Stats, CpuTimes)> {
    // The command prints the base readings, then `#net` and `/proc/net/dev`, then `#gpu` and
    // the graphics card readings. Each part is read on its own.
    let (base, extra) = output.split_once("#net").unwrap_or((output, ""));
    let (net_text, gpu_text) = extra.split_once("#gpu").unwrap_or((extra, ""));
    let output = base;
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

    let net = parse_net_dev(net_text);
    let gpu = parse_gpu(gpu_text);
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
            net_rx_bytes: net.map(|n| n.0),
            net_tx_bytes: net.map(|n| n.1),
            gpu_percent: gpu.map(|g| g.0),
            gpu_mem_used: gpu.map(|g| g.1),
            gpu_mem_total: gpu.map(|g| g.2),
        },
        now,
    ))
}

/// Asks `nvidia-smi` about this computer's graphics cards, if it has the program. No window is
/// shown on Windows, and a missing program just means no graphics reading.
fn read_local_gpu() -> Option<(f32, u64, u64)> {
    let mut command = std::process::Command::new("nvidia-smi");
    command
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_gpu(&String::from_utf8_lossy(&output.stdout))
}

/// Readings of this computer. Keep one around: the processor share needs the one before it.
pub struct LocalStats {
    system: sysinfo::System,
    /// False once `nvidia-smi` turned out not to be there, so it isn't tried every few seconds.
    gpu_possible: bool,
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
        Self {
            system,
            gpu_possible: true,
        }
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
        let networks = sysinfo::Networks::new_with_refreshed_list();
        let (mut rx, mut tx) = (0u64, 0u64);
        for (name, data) in &networks {
            let lower = name.to_lowercase();
            if lower == "lo" || lower.contains("loopback") {
                continue;
            }
            rx = rx.saturating_add(data.total_received());
            tx = tx.saturating_add(data.total_transmitted());
        }
        let gpu = if self.gpu_possible {
            let found = read_local_gpu();
            self.gpu_possible = found.is_some();
            found
        } else {
            None
        };
        Stats {
            cpu_percent: Some(self.system.global_cpu_usage()),
            mem_used: self.system.used_memory(),
            mem_total: self.system.total_memory(),
            disk_used,
            disk_total,
            load: None,
            uptime_secs: Some(sysinfo::System::uptime()),
            net_rx_bytes: Some(rx),
            net_tx_bytes: Some(tx),
            gpu_percent: gpu.map(|g| g.0),
            gpu_mem_used: gpu.map(|g| g.1),
            gpu_mem_total: gpu.map(|g| g.2),
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
    fn network_and_graphics_readings_are_parsed() {
        let net = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0\n  eth0: 5000 50 0 0 0 0 0 0 7000 70 0 0 0 0 0 0\n wlan0: 100 1 0 0 0 0 0 0 200 2 0 0 0 0 0 0\n";
        assert_eq!(parse_net_dev(net), Some((5100, 7200)));
        assert_eq!(parse_net_dev("nothing useful\n"), None);
        assert_eq!(
            parse_gpu("37, 1024, 8192\n80, 2048, 8192\n"),
            Some((80.0, 3072 * 1024 * 1024, 16384 * 1024 * 1024))
        );
        assert_eq!(parse_gpu(""), None);
        assert_eq!(parse_gpu("NVIDIA-SMI has failed\n"), None);
        let full = format!(
            "{SAMPLE_A}#net\n  eth0: 5000 50 0 0 0 0 0 0 7000 70 0 0 0 0 0 0\n#gpu\n12, 500, 4096\n"
        );
        let (stats, _) = parse_linux(&full, None).unwrap();
        assert_eq!(
            (stats.net_rx_bytes, stats.net_tx_bytes),
            (Some(5000), Some(7000))
        );
        assert_eq!(stats.gpu_percent, Some(12.0));
        assert_eq!(
            stats.mem_total,
            16_000_000 * 1024,
            "the extra sections must not disturb the base readings"
        );
        let (plain, _) = parse_linux(SAMPLE_A, None).unwrap();
        assert_eq!((plain.net_rx_bytes, plain.gpu_percent), (None, None));
    }

    #[test]
    fn this_computer_can_be_read() {
        let mut local = LocalStats::new();
        let stats = local.read(None);
        assert!(stats.mem_total > 0);
        assert!(stats.mem_used <= stats.mem_total);
    }
}
