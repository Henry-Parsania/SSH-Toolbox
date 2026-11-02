//! Docker: connecting (here or over a saved SSH session), the container list and its actions.
//! Logs and shells in containers are terminal sessions, opened by `terminal_open`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;
use toolbox_core::docker::engine::{self, Action, ContainerInfo, Details};
use toolbox_core::docker::resources;
use toolbox_core::docker::{Docker, connect_local, ssh::connect_ssh};
use toolbox_core::ssh::backend::SshServices;
use toolbox_core::terminal::TerminalStatus;
use toolbox_core::{AppError, ErrorCode};

use super::connect::shared_connection;
use super::keys::KeySetups;

/// The Docker engines that are connected, by ID. Panes that show a log or a shell in a
/// container look their engine up here.
#[derive(Default)]
pub struct Engines {
    open: Mutex<HashMap<String, Arc<Docker>>>,
}

impl Engines {
    pub fn get(&self, id: &str) -> Result<Arc<Docker>, AppError> {
        self.open.lock().unwrap().get(id).cloned().ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                "That Docker connection has been closed. Connect again.",
            )
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DockerTarget {
    /// Docker on this computer.
    Local,
    /// Docker on the server of a saved SSH session.
    Ssh { saved_session_id: String },
}

#[derive(Serialize)]
pub struct DockerOpened {
    pub docker_id: String,
}

#[tauri::command]
pub async fn docker_connect(
    engines: State<'_, Engines>,
    setups: State<'_, KeySetups>,
    services: State<'_, Arc<SshServices>>,
    target: DockerTarget,
    operation_id: String,
    on_status: Channel<TerminalStatus>,
) -> Result<DockerOpened, AppError> {
    let docker = match target {
        DockerTarget::Local => connect_local().await?,
        DockerTarget::Ssh { saved_session_id } => {
            let shared = shared_connection(
                &setups,
                &services,
                &operation_id,
                &saved_session_id,
                on_status,
            )
            .await?;
            connect_ssh(shared).await?.0
        }
    };
    let id = uuid::Uuid::new_v4().to_string();
    engines
        .open
        .lock()
        .unwrap()
        .insert(id.clone(), Arc::new(docker));
    Ok(DockerOpened { docker_id: id })
}

#[tauri::command]
pub fn docker_close(engines: State<'_, Engines>, docker_id: String) {
    engines.open.lock().unwrap().remove(&docker_id);
}

#[tauri::command]
pub async fn docker_list(
    engines: State<'_, Engines>,
    docker_id: String,
) -> Result<Vec<ContainerInfo>, AppError> {
    engine::list_containers(&*engines.get(&docker_id)?).await
}

#[tauri::command]
pub async fn docker_action(
    engines: State<'_, Engines>,
    docker_id: String,
    container_id: String,
    action: Action,
) -> Result<(), AppError> {
    engine::act(&*engines.get(&docker_id)?, &container_id, action).await
}

#[tauri::command]
pub async fn docker_inspect(
    engines: State<'_, Engines>,
    docker_id: String,
    container_id: String,
) -> Result<Details, AppError> {
    engine::inspect(&*engines.get(&docker_id)?, &container_id).await
}

/// Live processor and memory readings for the running containers.
#[tauri::command]
pub async fn docker_stats(
    engines: State<'_, Engines>,
    docker_id: String,
) -> Result<Vec<engine::ContainerStats>, AppError> {
    engine::stats(&*engines.get(&docker_id)?).await
}

#[tauri::command]
pub async fn docker_resources(
    engines: State<'_, Engines>,
    docker_id: String,
    kind: resources::Kind,
) -> Result<Vec<resources::Resource>, AppError> {
    resources::list(&*engines.get(&docker_id)?, kind).await
}

#[tauri::command]
pub async fn docker_rename(
    engines: State<'_, Engines>,
    docker_id: String,
    container_id: String,
    name: String,
) -> Result<(), AppError> {
    engine::rename(&*engines.get(&docker_id)?, &container_id, &name).await
}

#[tauri::command]
pub async fn docker_resource_details(
    engines: State<'_, Engines>,
    docker_id: String,
    kind: resources::Kind,
    id: String,
) -> Result<resources::ResourceDetails, AppError> {
    resources::inspect(&*engines.get(&docker_id)?, kind, &id).await
}

/// Clears out what is unused: stopped containers, untagged images, unused volumes or networks.
#[tauri::command]
pub async fn docker_prune(
    engines: State<'_, Engines>,
    docker_id: String,
    kind: resources::PruneKind,
) -> Result<resources::Pruned, AppError> {
    resources::prune(&*engines.get(&docker_id)?, kind).await
}

/// Downloads an image and waits until it is done.
#[tauri::command]
pub async fn docker_pull(
    engines: State<'_, Engines>,
    docker_id: String,
    reference: String,
    on_progress: Channel<resources::PullProgress>,
) -> Result<(), AppError> {
    resources::pull(&*engines.get(&docker_id)?, &reference, |p| {
        let _ = on_progress.send(p);
    })
    .await
}

/// Starts a new container from an image (downloading it first if needed). Returns its ID.
#[tauri::command]
pub async fn docker_run(
    engines: State<'_, Engines>,
    docker_id: String,
    spec: toolbox_core::docker::run::RunSpec,
) -> Result<String, AppError> {
    toolbox_core::docker::run::run(&*engines.get(&docker_id)?, &spec).await
}

/// Makes an empty volume or network.
#[tauri::command]
pub async fn docker_create_resource(
    engines: State<'_, Engines>,
    docker_id: String,
    kind: resources::Kind,
    name: String,
) -> Result<(), AppError> {
    resources::create(&*engines.get(&docker_id)?, kind, &name).await
}

#[tauri::command]
pub async fn docker_remove_resource(
    engines: State<'_, Engines>,
    docker_id: String,
    kind: resources::Kind,
    id: String,
) -> Result<(), AppError> {
    resources::remove(&*engines.get(&docker_id)?, kind, &id).await
}
