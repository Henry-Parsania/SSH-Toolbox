//! Images, volumes and networks: listing and removing. Like containers, everything Docker says is
//! cleaned before it is shown, and nothing is ever removed by force.

use bollard::Docker;
use bollard::query_parameters::{
    ListImagesOptionsBuilder, ListNetworksOptions, ListVolumesOptionsBuilder,
    RemoveImageOptionsBuilder, RemoveVolumeOptionsBuilder,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::engine::{check_id, clean};
use crate::error::{AppError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Images,
    Volumes,
    Networks,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Resource {
    /// What is passed back to remove it.
    pub id: String,
    pub name: String,
    pub detail: String,
    /// Docker's own networks can't be removed.
    pub protected: bool,
    /// Seconds since 1970 when it was made, if Docker says (images).
    pub created: Option<i64>,
}

fn failed(error: bollard::errors::Error) -> AppError {
    use bollard::errors::Error::DockerResponseServerError;
    match error {
        DockerResponseServerError {
            status_code: 404, ..
        } => AppError::new(ErrorCode::NotFound, "That no longer exists."),
        DockerResponseServerError {
            status_code: 409, ..
        } => AppError::new(
            ErrorCode::InvalidInput,
            "It's in use, so Docker won't remove it. Remove the containers that use it first.",
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

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(clean)
        .unwrap_or_default()
}

/// An image ID as Docker prints it: optional `sha256:` and hex.
fn check_image_id(id: &str) -> Result<(), AppError> {
    check_id(id.strip_prefix("sha256:").unwrap_or(id))
}

/// A volume name: letters, digits, underscore, dot and dash, not starting with a symbol.
pub(super) fn check_volume_name(name: &str) -> Result<(), AppError> {
    let ok = !name.is_empty()
        && name.len() <= 255
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if ok {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            "That isn't a volume name.",
        ))
    }
}

pub async fn list(docker: &Docker, kind: Kind) -> Result<Vec<Resource>, AppError> {
    let value = match kind {
        Kind::Images => serde_json::to_value(
            docker
                .list_images(Some(ListImagesOptionsBuilder::default().build()))
                .await
                .map_err(failed)?,
        ),
        Kind::Volumes => serde_json::to_value(
            docker
                .list_volumes(Some(ListVolumesOptionsBuilder::default().build()))
                .await
                .map_err(failed)?
                .volumes
                .unwrap_or_default(),
        ),
        Kind::Networks => serde_json::to_value(
            docker
                .list_networks(None::<ListNetworksOptions>)
                .await
                .map_err(failed)?,
        ),
    }
    .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read Docker's answer."))?;
    let mut out: Vec<Resource> = value
        .as_array()
        .map(|items| items.iter().filter_map(|v| describe(kind, v)).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn describe(kind: Kind, v: &Value) -> Option<Resource> {
    match kind {
        Kind::Images => {
            let id = text(v, "Id");
            if id.is_empty() {
                return None;
            }
            let name = v
                .get("RepoTags")
                .and_then(Value::as_array)
                .and_then(|t| t.first())
                .and_then(Value::as_str)
                .map(clean)
                .filter(|t| t != "<none>:<none>")
                .unwrap_or_else(|| "(untagged)".into());
            let size = v.get("Size").and_then(Value::as_u64).unwrap_or(0);
            let short: String = id.trim_start_matches("sha256:").chars().take(12).collect();
            Some(Resource {
                id,
                name,
                detail: format!("{short} · {}", megabytes(size)),
                protected: false,
                created: v.get("Created").and_then(Value::as_i64).filter(|c| *c > 0),
            })
        }
        Kind::Volumes => {
            let name = text(v, "Name");
            if name.is_empty() {
                return None;
            }
            Some(Resource {
                id: name.clone(),
                detail: format!("{} · {}", text(v, "Driver"), text(v, "Mountpoint")),
                name,
                protected: false,
                created: None,
            })
        }
        Kind::Networks => {
            let id = text(v, "Id");
            let name = text(v, "Name");
            if id.is_empty() {
                return None;
            }
            Some(Resource {
                protected: matches!(name.as_str(), "bridge" | "host" | "none"),
                detail: format!("{} · {}", text(v, "Driver"), text(v, "Scope")),
                id,
                name,
                created: None,
            })
        }
    }
}

/// The facts about one image, volume or network, as plain lines, and the complete answer.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResourceDetails {
    pub lines: Vec<String>,
    pub raw: String,
}

fn add(lines: &mut Vec<String>, label: &str, value: Option<String>) {
    if let Some(v) = value.map(|v| clean(&v)).filter(|v| !v.is_empty()) {
        lines.push(format!("{label}: {v}"));
    }
}

fn at(v: &Value, pointer: &str) -> Option<String> {
    match v.pointer(pointer)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "yes" } else { "no" }.into()),
        _ => None,
    }
}

fn list_at(v: &Value, pointer: &str) -> Option<String> {
    let items: Vec<String> = v
        .pointer(pointer)?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(clean)
        .collect();
    (!items.is_empty()).then(|| items.join(", "))
}

fn labels(v: &Value, pointer: &str, lines: &mut Vec<String>) {
    if let Some(map) = v.pointer(pointer).and_then(Value::as_object) {
        for (key, value) in map.iter().take(30) {
            lines.push(format!(
                "Label {}: {}",
                clean(key),
                value.as_str().map(clean).unwrap_or_default()
            ));
        }
    }
}

fn internal<E>(_: E) -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't read Docker's answer.")
}

pub async fn inspect(docker: &Docker, kind: Kind, id: &str) -> Result<ResourceDetails, AppError> {
    let value = match kind {
        Kind::Images => {
            check_image_id(id)?;
            serde_json::to_value(docker.inspect_image(id).await.map_err(failed)?)
                .map_err(internal)?
        }
        Kind::Volumes => {
            check_volume_name(id)?;
            serde_json::to_value(docker.inspect_volume(id).await.map_err(failed)?)
                .map_err(internal)?
        }
        Kind::Networks => {
            check_id(id)?;
            serde_json::to_value(
                docker
                    .inspect_network(id, None::<bollard::query_parameters::InspectNetworkOptions>)
                    .await
                    .map_err(failed)?,
            )
            .map_err(internal)?
        }
    };
    let mut lines = Vec::new();
    match kind {
        Kind::Images => {
            add(&mut lines, "Tags", list_at(&value, "/RepoTags"));
            add(&mut lines, "ID", at(&value, "/Id"));
            add(&mut lines, "Created", at(&value, "/Created"));
            add(
                &mut lines,
                "Size",
                value
                    .pointer("/Size")
                    .and_then(Value::as_u64)
                    .map(megabytes),
            );
            add(
                &mut lines,
                "System",
                at(&value, "/Os")
                    .zip(at(&value, "/Architecture"))
                    .map(|(o, a)| format!("{o}/{a}")),
            );
            add(&mut lines, "Command", list_at(&value, "/Config/Cmd"));
            add(
                &mut lines,
                "Entry point",
                list_at(&value, "/Config/Entrypoint"),
            );
            add(&mut lines, "Working folder", at(&value, "/Config/WorkingDir"));
            add(&mut lines, "User", at(&value, "/Config/User"));
            if let Some(ports) = value
                .pointer("/Config/ExposedPorts")
                .and_then(Value::as_object)
            {
                let mut names: Vec<String> = ports.keys().map(|k| clean(k)).collect();
                names.sort();
                add(&mut lines, "Ports", Some(names.join(", ")));
            }
            labels(&value, "/Config/Labels", &mut lines);
        }
        Kind::Volumes => {
            add(&mut lines, "Name", at(&value, "/Name"));
            add(&mut lines, "Driver", at(&value, "/Driver"));
            add(&mut lines, "Mounted at", at(&value, "/Mountpoint"));
            add(&mut lines, "Created", at(&value, "/CreatedAt"));
            add(&mut lines, "Scope", at(&value, "/Scope"));
            labels(&value, "/Labels", &mut lines);
        }
        Kind::Networks => {
            add(&mut lines, "Name", at(&value, "/Name"));
            add(&mut lines, "Driver", at(&value, "/Driver"));
            add(&mut lines, "Scope", at(&value, "/Scope"));
            add(&mut lines, "Created", at(&value, "/Created"));
            add(&mut lines, "Internal", at(&value, "/Internal"));
            if let Some(configs) = value.pointer("/IPAM/Config").and_then(Value::as_array) {
                for c in configs {
                    add(&mut lines, "Subnet", at(c, "/Subnet"));
                    add(&mut lines, "Gateway", at(c, "/Gateway"));
                }
            }
            if let Some(members) = value.get("Containers").and_then(Value::as_object) {
                for member in members.values() {
                    let line = format!(
                        "Container: {} {}",
                        text(member, "Name"),
                        text(member, "IPv4Address")
                    );
                    lines.push(line.trim_end().to_string());
                }
            }
            labels(&value, "/Labels", &mut lines);
        }
    }
    let mut raw = serde_json::to_string_pretty(&value).unwrap_or_default();
    if raw.len() > 256 * 1024 {
        let mut cut = 256 * 1024;
        while !raw.is_char_boundary(cut) {
            cut -= 1;
        }
        raw.truncate(cut);
        raw.push_str("\n… (cut)");
    }
    Ok(ResourceDetails { lines, raw })
}

/// What clearing out unused things removed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Pruned {
    pub removed: usize,
    pub bytes: u64,
}

/// What `prune` can clear. Containers are the stopped ones, images the untagged ones that nothing
/// uses, volumes the ones no container uses, networks the ones nothing is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PruneKind {
    Containers,
    Images,
    Volumes,
    Networks,
}

pub async fn prune(docker: &Docker, kind: PruneKind) -> Result<Pruned, AppError> {
    use bollard::query_parameters as q;
    let (value, key) = match kind {
        PruneKind::Containers => (
            serde_json::to_value(
                docker
                    .prune_containers(None::<q::PruneContainersOptions>)
                    .await
                    .map_err(failed)?,
            ),
            "ContainersDeleted",
        ),
        PruneKind::Images => (
            serde_json::to_value(
                docker
                    .prune_images(None::<q::PruneImagesOptions>)
                    .await
                    .map_err(failed)?,
            ),
            "ImagesDeleted",
        ),
        PruneKind::Volumes => (
            serde_json::to_value(
                docker
                    .prune_volumes(None::<q::PruneVolumesOptions>)
                    .await
                    .map_err(failed)?,
            ),
            "VolumesDeleted",
        ),
        PruneKind::Networks => (
            serde_json::to_value(
                docker
                    .prune_networks(None::<q::PruneNetworksOptions>)
                    .await
                    .map_err(failed)?,
            ),
            "NetworksDeleted",
        ),
    };
    let value = value.map_err(internal)?;
    Ok(Pruned {
        removed: value.get(key).and_then(Value::as_array).map_or(0, Vec::len),
        bytes: value
            .get("SpaceReclaimed")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

/// An image reference such as `nginx`, `nginx:1.27` or `ghcr.io/me/app:tag`.
pub(super) fn check_reference(reference: &str) -> Result<(), AppError> {
    let ok = !reference.is_empty()
        && reference.len() <= 255
        && reference
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && reference
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':' | '@'));
    if ok {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            "That isn't an image name. Try something like nginx:latest.",
        ))
    }
}

/// How far a download has got. The total is only known once Docker has said how big the parts are,
/// so it can grow while the download starts.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PullProgress {
    pub done: u64,
    pub total: u64,
    /// Docker's own word for the step, such as "Downloading" (cleaned).
    pub status: String,
}

/// Downloads an image, waiting until it is finished. `on_progress` is told how it is going, at
/// most about six times a second, and once more at the end.
pub async fn pull(
    docker: &Docker,
    reference: &str,
    on_progress: impl Fn(PullProgress),
) -> Result<(), AppError> {
    use futures_util::StreamExt;
    use std::collections::HashMap;
    check_reference(reference)?;
    let options = bollard::query_parameters::CreateImageOptionsBuilder::new()
        .from_image(reference)
        .build();
    let mut stream = Box::pin(docker.create_image(Some(options), None, None));
    let work = async {
        // Each layer of the image is downloaded on its own; this is how far each has got.
        let mut layers: HashMap<String, (u64, u64)> = HashMap::new();
        let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
        let mut status = String::new();
        while let Some(step) = stream.next().await {
            let step = step.map_err(failed)?;
            let value = serde_json::to_value(&step).map_err(internal)?;
            status = text(&value, "status");
            if let Some(id) = value.get("id").and_then(Value::as_str) {
                let current = value.pointer("/progressDetail/current").and_then(Value::as_u64);
                let total = value.pointer("/progressDetail/total").and_then(Value::as_u64);
                let entry = layers.entry(clean(id)).or_insert((0, 0));
                if let (Some(c), Some(t)) = (current, total) {
                    *entry = (c.min(t), t);
                }
                if status.contains("complete") {
                    entry.0 = entry.1;
                }
            }
            if last.elapsed() >= std::time::Duration::from_millis(150) {
                last = std::time::Instant::now();
                on_progress(PullProgress {
                    done: layers.values().map(|l| l.0).sum(),
                    total: layers.values().map(|l| l.1).sum(),
                    status: status.clone(),
                });
            }
        }
        let total = layers.values().map(|l| l.1).sum();
        on_progress(PullProgress { done: total, total, status });
        Ok(())
    };
    tokio::time::timeout(std::time::Duration::from_secs(900), work)
        .await
        .map_err(|_| AppError::new(ErrorCode::ConnectionFailed, "The download took too long."))?
}

pub async fn remove(docker: &Docker, kind: Kind, id: &str) -> Result<(), AppError> {
    match kind {
        Kind::Images => {
            check_image_id(id)?;
            docker
                .remove_image(
                    id,
                    Some(RemoveImageOptionsBuilder::default().force(false).build()),
                    None,
                )
                .await
                .map(|_| ())
                .map_err(failed)
        }
        Kind::Volumes => {
            check_volume_name(id)?;
            docker
                .remove_volume(
                    id,
                    Some(RemoveVolumeOptionsBuilder::default().force(false).build()),
                )
                .await
                .map_err(failed)
        }
        Kind::Networks => {
            check_id(id)?;
            docker.remove_network(id).await.map_err(failed)
        }
    }
}
