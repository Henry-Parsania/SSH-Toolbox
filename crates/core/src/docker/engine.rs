//! What the Docker screen does with an engine: list containers, change them, and read their
//! settings. Everything Docker says is shown as plain text, so names and values are cleaned of
//! control characters and cut to a sensible length first (a container can name itself anything).

use bollard::Docker;
use bollard::query_parameters::{
    InspectContainerOptionsBuilder, ListContainersOptionsBuilder, RemoveContainerOptionsBuilder,
    RestartContainerOptions, StartContainerOptions, StopContainerOptions,
};
use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, ErrorCode};

const MAX_TEXT: usize = 300;
/// Longest raw inspect output sent to the screen.
const MAX_RAW: usize = 512 * 1024;
const COMPOSE_PROJECT: &str = "com.docker.compose.project";
const COMPOSE_SERVICE: &str = "com.docker.compose.service";

/// Text from Docker made safe to show: control characters dropped, length limited.
pub fn clean(text: &str) -> String {
    let mut out: String = text.chars().filter(|c| !c.is_control()).collect();
    if out.chars().count() > MAX_TEXT {
        out = out.chars().take(MAX_TEXT).collect::<String>() + "…";
    }
    out
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    /// running, exited, paused, restarting, created, dead or removing.
    pub state: String,
    /// Docker's own wording, such as "Up 2 hours".
    pub status: String,
    /// The Compose project the container belongs to, if any.
    pub project: Option<String>,
    pub service: Option<String>,
    /// Published ports, such as "0.0.0.0:8080->80/tcp".
    pub ports: Vec<String>,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(clean)
        .unwrap_or_default()
}

fn label(value: &Value, key: &str) -> Option<String> {
    value
        .get("Labels")
        .and_then(|l| l.get(key))
        .and_then(Value::as_str)
        .map(clean)
        .filter(|s| !s.is_empty())
}

fn port_text(port: &Value) -> Option<String> {
    let private = port.get("PrivatePort")?.as_u64()?;
    let kind = port.get("Type").and_then(Value::as_str).unwrap_or("tcp");
    Some(match port.get("PublicPort").and_then(Value::as_u64) {
        Some(public) => {
            let ip = port.get("IP").and_then(Value::as_str).unwrap_or("");
            let ip = clean(ip);
            if ip.is_empty() {
                format!("{public}->{private}/{}", clean(kind))
            } else {
                format!("{ip}:{public}->{private}/{}", clean(kind))
            }
        }
        None => format!("{private}/{}", clean(kind)),
    })
}

fn failed(error: bollard::errors::Error) -> AppError {
    use bollard::errors::Error::*;
    match error {
        DockerResponseServerError {
            status_code: 404, ..
        } => AppError::new(ErrorCode::NotFound, "That container no longer exists."),
        DockerResponseServerError {
            status_code: 409, ..
        } => AppError::new(
            ErrorCode::InvalidInput,
            "Docker can't do that to the container in its current state.",
        ),
        DockerResponseServerError {
            status_code: 403, ..
        } => AppError::new(
            ErrorCode::PermissionDenied,
            "Docker refused. This user may not be allowed to do that.",
        ),
        DockerResponseServerError { .. } => {
            AppError::new(ErrorCode::Internal, "Docker couldn't do that.")
        }
        _ => AppError::new(
            ErrorCode::ConnectionLost,
            "The connection to Docker was lost.",
        ),
    }
}

/// Every container, running or not, newest first.
pub async fn list_containers(docker: &Docker) -> Result<Vec<ContainerInfo>, AppError> {
    let all = docker
        .list_containers(Some(
            ListContainersOptionsBuilder::default().all(true).build(),
        ))
        .await
        .map_err(failed)?;
    let mut out: Vec<(i64, ContainerInfo)> = all
        .iter()
        .filter_map(|summary| {
            let value = serde_json::to_value(summary).ok()?;
            let id = text(&value, "Id");
            if id.is_empty() {
                return None;
            }
            let name = value
                .get("Names")
                .and_then(Value::as_array)
                .and_then(|n| n.first())
                .and_then(Value::as_str)
                .map(|n| clean(n.trim_start_matches('/')))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| id.chars().take(12).collect());
            let created = value.get("Created").and_then(Value::as_i64).unwrap_or(0);
            Some((
                created,
                ContainerInfo {
                    id,
                    name,
                    image: text(&value, "Image"),
                    state: text(&value, "State"),
                    status: text(&value, "Status"),
                    project: label(&value, COMPOSE_PROJECT),
                    service: label(&value, COMPOSE_SERVICE),
                    ports: value
                        .get("Ports")
                        .and_then(Value::as_array)
                        .map(|p| p.iter().filter_map(port_text).collect())
                        .unwrap_or_default(),
                },
            ))
        })
        .collect();
    out.sort_by_key(|(created, _)| std::cmp::Reverse(*created));
    Ok(out.into_iter().map(|(_, info)| info).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Start,
    Stop,
    Restart,
    /// Removes a stopped container. A running one is never removed by force.
    Remove,
}

/// A container ID is hex. Anything else is refused before it can end up in a request path.
pub fn check_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That isn't a container ID.",
        ));
    }
    Ok(())
}

pub async fn act(docker: &Docker, id: &str, action: Action) -> Result<(), AppError> {
    check_id(id)?;
    match action {
        Action::Start => docker
            .start_container(id, None::<StartContainerOptions>)
            .await
            .map_err(failed),
        Action::Stop => docker
            .stop_container(id, None::<StopContainerOptions>)
            .await
            .map_err(failed),
        Action::Restart => docker
            .restart_container(id, None::<RestartContainerOptions>)
            .await
            .map_err(failed),
        Action::Remove => docker
            .remove_container(
                id,
                Some(
                    RemoveContainerOptionsBuilder::default()
                        .force(false)
                        .build(),
                ),
            )
            .await
            .map_err(failed),
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct Details {
    pub name: String,
    pub image: String,
    pub state: String,
    pub tty: bool,
    pub env: Vec<String>,
    pub ports: Vec<String>,
    pub mounts: Vec<String>,
    pub networks: Vec<String>,
    /// The complete `docker inspect` output, formatted. Shown as text only.
    pub raw: String,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(clean).collect())
        .unwrap_or_default()
}

pub async fn inspect(docker: &Docker, id: &str) -> Result<Details, AppError> {
    check_id(id)?;
    let response = docker
        .inspect_container(id, Some(InspectContainerOptionsBuilder::default().build()))
        .await
        .map_err(failed)?;
    let value = serde_json::to_value(&response)
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read Docker's answer."))?;

    let ports = value
        .pointer("/NetworkSettings/Ports")
        .and_then(Value::as_object)
        .map(|map| {
            let mut lines: Vec<String> = map
                .iter()
                .map(|(inside, bindings)| {
                    let to: Vec<String> = bindings
                        .as_array()
                        .map(|b| {
                            b.iter()
                                .map(|b| {
                                    format!(
                                        "{}:{}",
                                        b.get("HostIp")
                                            .and_then(Value::as_str)
                                            .map(clean)
                                            .unwrap_or_default(),
                                        b.get("HostPort")
                                            .and_then(Value::as_str)
                                            .map(clean)
                                            .unwrap_or_default()
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if to.is_empty() {
                        clean(inside)
                    } else {
                        format!("{} -> {}", to.join(", "), clean(inside))
                    }
                })
                .collect();
            lines.sort();
            lines
        })
        .unwrap_or_default();

    let mounts = value
        .get("Mounts")
        .and_then(Value::as_array)
        .map(|m| {
            m.iter()
                .map(|m| {
                    let from = m
                        .get("Source")
                        .or_else(|| m.get("Name"))
                        .and_then(Value::as_str)
                        .map(clean)
                        .unwrap_or_default();
                    let to = text(m, "Destination");
                    let mode = if m.get("RW").and_then(Value::as_bool) == Some(false) {
                        " (read-only)"
                    } else {
                        ""
                    };
                    format!("{from} -> {to}{mode}")
                })
                .collect()
        })
        .unwrap_or_default();

    let networks = value
        .pointer("/NetworkSettings/Networks")
        .and_then(Value::as_object)
        .map(|map| {
            let mut lines: Vec<String> = map
                .iter()
                .map(|(name, net)| {
                    let ip = text(net, "IPAddress");
                    if ip.is_empty() {
                        clean(name)
                    } else {
                        format!("{} ({ip})", clean(name))
                    }
                })
                .collect();
            lines.sort();
            lines
        })
        .unwrap_or_default();

    let mut raw = serde_json::to_string_pretty(&value).unwrap_or_default();
    if raw.len() > MAX_RAW {
        let mut cut = MAX_RAW;
        while !raw.is_char_boundary(cut) {
            cut -= 1;
        }
        raw.truncate(cut);
        raw.push_str("\n… (cut)");
    }

    Ok(Details {
        name: clean(text(&value, "Name").trim_start_matches('/')),
        image: text(&value, "Image"),
        state: value
            .pointer("/State/Status")
            .and_then(Value::as_str)
            .map(clean)
            .unwrap_or_default(),
        tty: value
            .pointer("/Config/Tty")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        env: strings(value.pointer("/Config/Env")),
        ports,
        mounts,
        networks,
        raw,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContainerStats {
    pub id: String,
    /// Share of the computer's processors in use, in percent (100 is one full core).
    pub cpu_percent: f64,
    pub memory_used: u64,
    pub memory_limit: u64,
}

/// Reads one Docker stats sample. Returns none if it has no readings (a container that just
/// stopped, say).
pub fn read_stats(id: &str, sample: &Value) -> Option<ContainerStats> {
    let number = |path: &str| sample.pointer(path).and_then(Value::as_f64);
    let used = number("/memory_stats/usage")?;
    // Like `docker stats`, file cache the system can take back doesn't count as used.
    let cache = number("/memory_stats/stats/inactive_file")
        .or_else(|| number("/memory_stats/stats/cache"))
        .unwrap_or(0.0);
    let cpu_delta = number("/cpu_stats/cpu_usage/total_usage")?
        - number("/precpu_stats/cpu_usage/total_usage")?;
    let system_delta =
        number("/cpu_stats/system_cpu_usage")? - number("/precpu_stats/system_cpu_usage")?;
    let cpus = number("/cpu_stats/online_cpus")
        .filter(|c| *c > 0.0)
        .unwrap_or(1.0);
    let cpu_percent = if system_delta > 0.0 && cpu_delta >= 0.0 {
        cpu_delta / system_delta * cpus * 100.0
    } else {
        0.0
    };
    Some(ContainerStats {
        id: id.to_string(),
        cpu_percent,
        memory_used: (used - cache).max(0.0) as u64,
        memory_limit: number("/memory_stats/limit").unwrap_or(0.0) as u64,
    })
}

/// How many containers are read at once, and in total, so a busy server isn't flooded.
const STATS_AT_ONCE: usize = 8;
const STATS_MAX: usize = 40;

/// Live readings for the running containers. A container that can't be read is left out.
pub async fn stats(docker: &Docker) -> Result<Vec<ContainerStats>, AppError> {
    use futures_util::StreamExt;
    let running: Vec<String> = list_containers(docker)
        .await?
        .into_iter()
        .filter(|c| c.state == "running")
        .map(|c| c.id)
        .take(STATS_MAX)
        .collect();
    let readings = futures_util::stream::iter(running.into_iter().map(|id| async move {
        let options = bollard::query_parameters::StatsOptionsBuilder::default()
            .stream(false)
            .build();
        let mut stream = Box::pin(docker.stats(&id, Some(options)));
        let sample = stream.next().await?.ok()?;
        let value = serde_json::to_value(&sample).ok()?;
        read_stats(&id, &value)
    }))
    .buffer_unordered(STATS_AT_ONCE)
    .collect::<Vec<_>>()
    .await;
    Ok(readings.into_iter().flatten().collect())
}
