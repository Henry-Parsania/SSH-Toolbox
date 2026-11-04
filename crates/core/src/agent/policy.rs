//! What the agent may run on a server, decided before anything is sent. This is the hard part of
//! the safety: the interface asks the user, but these rules can't be switched off by an approval.
//!
//! * `Blocked`: never run, whoever asks (wiping the disk, `rm -rf /`, fork bombs...).
//! * `ReadOnly`: only looks at things, so it may be run without asking if the user has allowed that.
//! * `Changes`: anything else. Always needs the user's approval.
//!
//! The reading rules are deliberately narrow: a command is read-only only if every part of it is a
//! known harmless command with no way to run another command or write a file. When in doubt it is
//! `Changes`, which only costs one click.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    ReadOnly,
    Changes,
    Blocked,
}

/// Commands that only look. Each is checked further below for flags that make them do more.
const READ_ONLY: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "df",
    "du",
    "ps",
    "free",
    "uptime",
    "uname",
    "whoami",
    "id",
    "pwd",
    "grep",
    "egrep",
    "fgrep",
    "stat",
    "wc",
    "date",
    "hostname",
    "hostnamectl",
    "lsblk",
    "lscpu",
    "lsmod",
    "lspci",
    "lsusb",
    "env",
    "printenv",
    "which",
    "whereis",
    "type",
    "file",
    "sort",
    "uniq",
    "cut",
    "tr",
    "nl",
    "less",
    "more",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "echo",
    "journalctl",
    "dmesg",
    "ss",
    "netstat",
    "ip",
    "ping",
    "dig",
    "nslookup",
    "last",
    "who",
    "w",
    "top",
    "vmstat",
    "iostat",
    "tree",
    "md5sum",
    "sha256sum",
    "sha1sum",
    "find",
    "docker",
    "systemctl",
    "git",
    "uptime",
    "nproc",
    "arch",
    "locale",
    "lsof",
    "pgrep",
];

/// Parts of a command that always make it risky, wherever they appear.
const DANGEROUS_SYNTAX: &[&str] = &[";", "&", ">", "<", "`", "$(", "\n", "\r", "\\"];

/// Things that are never run.
fn blocked(command: &str) -> bool {
    let c = command.to_ascii_lowercase();
    let squashed: String = c.split_whitespace().collect::<Vec<_>>().join(" ");
    let has = |s: &str| squashed.contains(s);
    // Deleting the top of the file system, or everything under it.
    let rm_root = squashed.split(['|', ';', '&']).any(|part| {
        let words: Vec<&str> = part.split_whitespace().collect();
        let rm = words.iter().position(|w| *w == "rm" || w.ends_with("/rm"));
        match rm {
            Some(at) => {
                let args = &words[at + 1..];
                let recursive = args.iter().any(|a| {
                    a.starts_with('-') && (a.contains('r') || a.contains('R'))
                        || *a == "--recursive"
                });
                let target_root = args.iter().any(|a| {
                    matches!(
                        *a,
                        "/" | "/*"
                            | "~"
                            | "~/"
                            | "~/*"
                            | "$home"
                            | "$home/"
                            | "$home/*"
                            | "/home"
                            | "/home/*"
                            | "/etc"
                            | "/etc/*"
                            | "/usr"
                            | "/usr/*"
                            | "/var"
                            | "/var/*"
                            | "/boot"
                            | "/bin"
                            | "/lib"
                            | "/root"
                    ) || *a == "--no-preserve-root"
                });
                recursive && target_root
            }
            None => false,
        }
    });
    rm_root
        || has("mkfs")
        || has("dd if=") && has("of=/dev/")
        || has("> /dev/sd")
        || has("> /dev/nvme")
        || has(":(){")
        || has("chmod -r 777 /")
        || has("chown -r")
            && has(" /")
            && !has(" /home/")
            && !has(" /srv/")
            && !has(" /var/www")
            && !has(" /opt/")
        || has("shred ") && has("/dev/")
        || has("wipefs")
        || has("rm -rf --no-preserve-root")
}

/// The first word of a command, without a path or an `sudo`-style prefix.
fn base_name(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

/// Whether one part of a pipeline (a single command) only looks at things.
fn part_is_read_only(part: &str) -> bool {
    let words: Vec<&str> = part.split_whitespace().collect();
    let Some(first) = words.first() else {
        return false;
    };
    let name = base_name(first);
    if !READ_ONLY.contains(&name) {
        return false;
    }
    let args = &words[1..];
    let has_arg = |a: &str| args.contains(&a);
    let starts = |p: &str| args.iter().any(|x| x.starts_with(p));
    match name {
        // These can run or delete things.
        "find" => {
            !(starts("-exec")
                || starts("-ok")
                || has_arg("-delete")
                || starts("-fprint")
                || starts("-fls"))
        }
        "sort" => !(starts("-o") || starts("--output")),
        "less" | "more" | "top" => false, // interactive programs can't be used without a screen
        // `env NAME=value command` runs a command.
        "env" => args.is_empty(),
        "tree" => !starts("-o"),
        "ip" => !args.iter().any(|a| {
            matches!(
                *a,
                "add" | "del" | "delete" | "set" | "flush" | "replace" | "change"
            )
        }),
        "ping" => has_arg("-c") || starts("-c"),
        "systemctl" => args.iter().find(|a| !a.starts_with('-')).is_some_and(|a| {
            matches!(
                *a,
                "status"
                    | "is-active"
                    | "is-enabled"
                    | "is-failed"
                    | "list-units"
                    | "list-unit-files"
                    | "list-timers"
                    | "show"
                    | "cat"
            )
        }),
        "docker" => {
            args.iter().find(|a| !a.starts_with('-')).is_some_and(|a| {
                matches!(
                    *a,
                    "ps" | "images"
                        | "logs"
                        | "inspect"
                        | "stats"
                        | "version"
                        | "info"
                        | "volume"
                        | "network"
                        | "compose"
                )
            }) && !args.iter().any(|a| {
                matches!(
                    *a,
                    "rm" | "prune"
                        | "create"
                        | "up"
                        | "down"
                        | "start"
                        | "stop"
                        | "restart"
                        | "kill"
                        | "run"
                        | "exec"
                        | "pull"
                        | "build"
                )
            })
        }
        "git" => {
            args.iter().find(|a| !a.starts_with('-')).is_some_and(|a| {
                matches!(
                    *a,
                    "status"
                        | "log"
                        | "diff"
                        | "show"
                        | "branch"
                        | "remote"
                        | "describe"
                        | "rev-parse"
                        | "ls-files"
                )
            }) && !args
                .iter()
                .any(|a| matches!(*a, "-D" | "-d" | "--delete" | "-m" | "-M" | "-c" | "-C"))
        }
        "journalctl" => {
            !starts("--vacuum") && !starts("--rotate") && !starts("--flush") && !starts("--sync")
        }
        "dmesg" => !(has_arg("-c") || has_arg("-C") || starts("--clear") || starts("--console")),
        "date" => !(has_arg("-s") || starts("--set")),
        "hostname" => args.is_empty() || args.iter().all(|a| a.starts_with('-') && *a != "-b"),
        "hostnamectl" => args.is_empty() || args.first().is_some_and(|a| *a == "status"),
        "dig" | "nslookup" | "ss" | "netstat" | "lsof" | "pgrep" => true,
        _ => true,
    }
}

/// How risky a command line is.
pub fn classify(command: &str) -> Risk {
    let command = command.trim();
    if command.is_empty() {
        return Risk::Changes;
    }
    if blocked(command) {
        return Risk::Blocked;
    }
    if DANGEROUS_SYNTAX.iter().any(|s| command.contains(s)) {
        return Risk::Changes;
    }
    // `||` is the only way left to join commands; pipes are allowed between read-only ones.
    if command.contains("||") {
        return Risk::Changes;
    }
    // Looking at files that hold secrets is allowed, but only with the user's say-so.
    let lower = command.to_ascii_lowercase();
    if [
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        "/etc/shadow",
        ".env",
        ".pem",
        ".kdbx",
        "credentials",
        ".netrc",
        ".pgpass",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        return Risk::Changes;
    }
    if command
        .split('|')
        .all(|part| part_is_read_only(part.trim()))
    {
        Risk::ReadOnly
    } else {
        Risk::Changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looking_around_is_read_only() {
        for c in [
            "ls -la /var/log",
            "cat /etc/os-release",
            "df -h",
            "ps aux | grep nginx",
            "journalctl -u nginx -n 100 --no-pager",
            "systemctl status nginx",
            "docker ps -a",
            "git log --oneline -5",
            "free -m",
            "find /var/log -name '*.log' -mtime -1",
            "tail -n 50 /var/log/syslog | grep error",
        ] {
            assert_eq!(classify(c), Risk::ReadOnly, "{c}");
        }
    }

    #[test]
    fn anything_that_could_change_things_needs_approval() {
        for c in [
            "rm file.txt",
            "systemctl restart nginx",
            "docker rm web",
            "docker run -it ubuntu",
            "echo hi > /etc/motd",
            "ls; rm x",
            "ls && reboot",
            "cat /etc/passwd | sudo tee /x",
            "find / -name x -delete",
            "find . -exec rm {} +",
            "sort -o out.txt in.txt",
            "ls $(whoami)",
            "ls `whoami`",
            "ip link set eth0 down",
            "git push",
            "git branch -D old",
            "date -s 12:00",
            "journalctl --vacuum-size=1M",
            "apt install nginx",
            "sudo ls",
            "python3 -c 'print(1)'",
            "",
            "ping example.com",
            "cat ~/.ssh/id_ed25519",
            "cat /etc/shadow",
            "env FOO=1 ls",
            "ls\nrm x",
        ] {
            assert_eq!(classify(c), Risk::Changes, "{c:?}");
        }
    }

    #[test]
    fn the_worst_commands_are_blocked_outright() {
        for c in [
            "rm -rf /",
            "rm -rf /*",
            "sudo rm -rf / --no-preserve-root",
            "rm -fr ~",
            "rm -r -f /etc",
            "mkfs.ext4 /dev/sda1",
            "dd if=/dev/zero of=/dev/sda",
            ":(){ :|:& };:",
            "echo x > /dev/sda",
            "chmod -R 777 /",
            "ls | rm -rf /",
            "wipefs -a /dev/sda",
        ] {
            assert_eq!(classify(c), Risk::Blocked, "{c}");
        }
        // Deleting inside a folder is a normal change, not blocked.
        assert_eq!(classify("rm -rf /tmp/build"), Risk::Changes);
        assert_eq!(classify("rm -rf ./node_modules"), Risk::Changes);
    }
}
