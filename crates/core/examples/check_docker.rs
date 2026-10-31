//! Library check: bollard pings Docker and lists containers.
use bollard::Docker;
use bollard::query_parameters::ListContainersOptionsBuilder;

#[tokio::main]
async fn main() {
    let docker = Docker::connect_with_local_defaults().expect("connect");
    let pong = docker.ping().await.expect("ping");
    let containers = docker
        .list_containers(Some(
            ListContainersOptionsBuilder::default().all(true).build(),
        ))
        .await
        .expect("list containers");
    println!(
        "bollard OK: ping={pong:?}, {} container(s)",
        containers.len()
    );
}
