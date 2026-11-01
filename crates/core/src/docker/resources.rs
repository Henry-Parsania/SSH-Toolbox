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
fn check_volume_name(name: &str) -> Result<(), AppError> {
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
            })
        }
    }
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
