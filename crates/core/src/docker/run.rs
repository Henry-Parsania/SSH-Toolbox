//! Starting a new container from an image: the "docker run" of the screen. Everything typed in
//! the dialog is checked here before it becomes part of a request, and the body is built as data,
//! never as text for a shell, so nothing typed can add options or commands.

use bollard::Docker;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptionsBuilder, StartContainerOptions};
use serde::Deserialize;
use serde_json::{Value, json};

use super::engine::{check_id, check_name};
use super::resources::{check_reference, check_volume_name, pull};
use crate::error::{AppError, ErrorCode};

const MAX_ITEMS: usize = 64;

#[derive(Debug, Clone, Deserialize)]
pub struct PortSpec {
    /// The port on the host computer.
    pub host: u16,
    /// The port inside the container.
    pub container: u16,
    #[serde(default)]
    pub udp: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolumeSpec {
    /// A folder on the host (absolute path) or the name of a Docker volume.
    pub source: String,
    /// Where it appears inside the container (absolute path).
    pub target: String,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunSpec {
    pub image: String,
    /// Optional. Docker picks one when empty.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ports: Vec<PortSpec>,
    /// Publish ports on 127.0.0.1 only, instead of on every network the host is on.
    #[serde(default = "yes")]
    pub localhost_only: bool,
    #[serde(default)]
    pub volumes: Vec<VolumeSpec>,
    /// `NAME=value` lines.
    #[serde(default)]
    pub env: Vec<String>,
    /// no, always, unless-stopped or on-failure.
    #[serde(default = "no")]
    pub restart: String,
}

fn yes() -> bool {
    true
}

fn no() -> String {
    "no".into()
}

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

fn plain(text: &str) -> bool {
    !text.contains('\0') && !text.contains('\n') && !text.contains('\r')
}

/// A folder on the host: `/srv/data`, or `C:\Data` on Windows.
fn is_host_path(source: &str) -> bool {
    let bytes = source.as_bytes();
    source.starts_with('/')
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// Checks the request and builds what Docker is sent: the name to give, and the body.
pub fn build(spec: &RunSpec) -> Result<(Option<String>, Value), AppError> {
    check_reference(&spec.image)?;
    let name = spec.name.trim();
    if !name.is_empty() {
        check_name(name)?;
    }
    if spec.ports.len() > MAX_ITEMS || spec.volumes.len() > MAX_ITEMS || spec.env.len() > MAX_ITEMS
    {
        return Err(invalid("That is too many ports, folders or variables."));
    }

    let mut exposed = serde_json::Map::new();
    let mut bindings = serde_json::Map::new();
    for port in &spec.ports {
        if port.host == 0 || port.container == 0 {
            return Err(invalid("Ports are numbers from 1 to 65535."));
        }
        let key = format!("{}/{}", port.container, if port.udp { "udp" } else { "tcp" });
        exposed.insert(key.clone(), json!({}));
        let entry = bindings.entry(key).or_insert_with(|| json!([]));
        let host_ip = if spec.localhost_only { "127.0.0.1" } else { "" };
        entry
            .as_array_mut()
            .expect("an array")
            .push(json!({ "HostIp": host_ip, "HostPort": port.host.to_string() }));
    }

    let mut mounts = Vec::new();
    for volume in &spec.volumes {
        let target = volume.target.trim();
        if !target.starts_with('/') || !plain(target) || target == "/" {
            return Err(invalid("Inside the container, use a full path such as /data."));
        }
        let source = volume.source.trim();
        let kind = if is_host_path(source) && plain(source) {
            "bind"
        } else {
            check_volume_name(source).map_err(|_| {
                invalid("A folder needs a full path such as /srv/data; a volume needs its name.")
            })?;
            "volume"
        };
        mounts.push(json!({
            "Type": kind,
            "Source": source,
            "Target": target,
            "ReadOnly": volume.read_only,
        }));
    }

    for line in &spec.env {
        let key = line.split('=').next().unwrap_or("");
        let ok = line.contains('=')
            && plain(line)
            && key
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ok {
            return Err(invalid("Variables are written NAME=value, one for each line."));
        }
    }

    let restart = match spec.restart.as_str() {
        "no" | "always" | "unless-stopped" | "on-failure" => spec.restart.as_str(),
        _ => return Err(invalid("Choose no, always, unless-stopped or on-failure.")),
    };

    let mut body = json!({
        "Image": spec.image,
        "HostConfig": {
            "Mounts": mounts,
            "RestartPolicy": { "Name": restart },
        },
    });
    if !spec.env.is_empty() {
        body["Env"] = json!(spec.env);
    }
    if !spec.ports.is_empty() {
        body["ExposedPorts"] = Value::Object(exposed);
        body["HostConfig"]["PortBindings"] = Value::Object(bindings);
    }
    Ok(((!name.is_empty()).then(|| name.to_string()), body))
}

/// Says what went wrong from Docker's own words, without showing them.
fn explain(error: bollard::errors::Error) -> AppError {
    use bollard::errors::Error::DockerResponseServerError;
    match error {
        DockerResponseServerError {
            status_code: 409, ..
        } => invalid("A container with that name already exists."),
        DockerResponseServerError {
            status_code: 404, ..
        } => AppError::new(
            ErrorCode::NotFound,
            "Docker doesn't have that image, and couldn't download it.",
        ),
        DockerResponseServerError { message, .. } => {
            let text = message.to_lowercase();
            if text.contains("already allocated") || text.contains("address already in use") {
                invalid("One of those ports is already in use on this computer.")
            } else if text.contains("bind source path does not exist")
                || text.contains("no such file")
                || text.contains("mkdir")
            {
                invalid("A folder you gave doesn't exist on the host.")
            } else if text.contains("denied") || text.contains("permission") {
                AppError::new(
                    ErrorCode::PermissionDenied,
                    "Docker refused. This user may not be allowed to do that.",
                )
            } else {
                AppError::new(ErrorCode::Internal, "Docker couldn't start that container.")
            }
        }
        _ => AppError::new(
            ErrorCode::ConnectionLost,
            "The connection to Docker was lost.",
        ),
    }
}

/// Creates a container from the image and starts it. If the image isn't on the host yet it is
/// downloaded first. A container that can't be started is removed again. Returns its ID.
pub async fn run(docker: &Docker, spec: &RunSpec) -> Result<String, AppError> {
    let (name, body) = build(spec)?;
    let body: ContainerCreateBody = serde_json::from_value(body)
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't build that request."))?;

    if docker.inspect_image(&spec.image).await.is_err() {
        pull(docker, &spec.image, |_| {}).await?;
    }

    let options = name
        .as_deref()
        .map(|n| CreateContainerOptionsBuilder::new().name(n).build());
    let created = docker
        .create_container(options, body)
        .await
        .map_err(explain)?;
    check_id(&created.id)?;
    if let Err(error) = docker
        .start_container(&created.id, None::<StartContainerOptions>)
        .await
    {
        let _ = docker.remove_container(&created.id, None).await;
        return Err(explain(error));
    }
    Ok(created.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> RunSpec {
        RunSpec {
            image: "nginx:1.27".into(),
            name: "web".into(),
            ports: vec![PortSpec { host: 8080, container: 80, udp: false }],
            localhost_only: true,
            volumes: vec![
                VolumeSpec { source: "/srv/site".into(), target: "/usr/share/nginx/html".into(), read_only: true },
                VolumeSpec { source: "cache".into(), target: "/cache".into(), read_only: false },
            ],
            env: vec!["MODE=prod".into()],
            restart: "unless-stopped".into(),
        }
    }

    #[test]
    fn a_good_request_becomes_data_not_text() {
        let (name, body) = build(&spec()).unwrap();
        assert_eq!(name.as_deref(), Some("web"));
        assert_eq!(body["Image"], "nginx:1.27");
        assert_eq!(body["HostConfig"]["PortBindings"]["80/tcp"][0]["HostIp"], "127.0.0.1");
        assert_eq!(body["HostConfig"]["PortBindings"]["80/tcp"][0]["HostPort"], "8080");
        assert_eq!(body["HostConfig"]["Mounts"][0]["Type"], "bind");
        assert_eq!(body["HostConfig"]["Mounts"][0]["ReadOnly"], true);
        assert_eq!(body["HostConfig"]["Mounts"][1]["Type"], "volume");
        assert_eq!(body["HostConfig"]["RestartPolicy"]["Name"], "unless-stopped");
        assert_eq!(body["Env"][0], "MODE=prod");
        let parsed: Result<ContainerCreateBody, _> = serde_json::from_value(body);
        assert!(parsed.is_ok());
    }

    #[test]
    fn ports_are_open_to_every_network_only_when_asked() {
        let mut s = spec();
        s.localhost_only = false;
        let (_, body) = build(&s).unwrap();
        assert_eq!(body["HostConfig"]["PortBindings"]["80/tcp"][0]["HostIp"], "");
    }

    #[test]
    fn anything_that_is_not_what_it_says_is_refused() {
        let cases: Vec<Box<dyn Fn(&mut RunSpec)>> = vec![
            Box::new(|s| s.image = "--privileged".into()),
            Box::new(|s| s.image = "nginx x".into()),
            Box::new(|s| s.name = "a b".into()),
            Box::new(|s| s.ports[0].host = 0),
            Box::new(|s| s.volumes[0].target = "relative".into()),
            Box::new(|s| s.volumes[0].target = "/".into()),
            Box::new(|s| s.volumes[0].source = "a/b".into()),
            Box::new(|s| s.volumes[1].source = "../x".into()),
            Box::new(|s| s.env = vec!["NOEQUALS".into()]),
            Box::new(|s| s.env = vec!["1BAD=x".into()]),
            Box::new(|s| s.env = vec!["A B=x".into()]),
            Box::new(|s| s.env = vec!["A=x\nB=y".into()]),
            Box::new(|s| s.restart = "sometimes".into()),
        ];
        for change in cases {
            let mut s = spec();
            change(&mut s);
            assert_eq!(build(&s).unwrap_err().code, ErrorCode::InvalidInput);
        }
    }

    #[test]
    fn windows_folders_count_as_host_paths() {
        assert!(is_host_path(r"C:\Data"));
        assert!(is_host_path("D:/x"));
        assert!(!is_host_path("data"));
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;
    use crate::docker::fake::fake_docker_engine;

    #[tokio::test]
    async fn running_creates_then_starts_with_the_checked_body() {
        let (port, seen) = fake_docker_engine().await;
        let docker = Docker::connect_with_http(
            &format!("http://127.0.0.1:{port}"),
            10,
            bollard::API_DEFAULT_VERSION,
        )
        .unwrap();
        let spec = RunSpec {
            image: "nginx".into(),
            name: "web".into(),
            ports: vec![PortSpec { host: 8080, container: 80, udp: false }],
            localhost_only: true,
            volumes: vec![],
            env: vec!["A=1".into()],
            restart: "no".into(),
        };
        let id = run(&docker, &spec).await.unwrap();
        assert_eq!(id, "abc123");
        let seen = seen.lock().unwrap().join("\n");
        assert!(seen.contains("POST /v1.52/containers/create?name=web") || seen.contains("/containers/create?name=web"), "{seen}");
        assert!(seen.contains(r#""HostPort":"8080""#), "{seen}");
        assert!(seen.contains("/containers/abc123/start"), "{seen}");
    }
}
