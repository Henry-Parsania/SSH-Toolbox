//! Library check: starting a container from the app's own "run" code against the Docker on this
//! computer. Usage: `cargo run -p toolbox-core --example check_docker_run -- <image already on
//! this computer>`. Nothing is downloaded: an image that isn't here makes the check stop. It makes
//! one container and one volume with a `toolbox-check-` name and removes both at the end.
use toolbox_core::docker::engine::{self, Action};
use toolbox_core::docker::resources::{self, Kind};
use toolbox_core::docker::run::{PortSpec, RunSpec, VolumeSpec, run};
use toolbox_core::docker::connect_local;

#[tokio::main]
async fn main() {
    let image = std::env::args().nth(1).expect("give an image that is already here");
    let docker = connect_local().await.expect("Docker is not running");
    docker
        .inspect_image(&image)
        .await
        .expect("that image isn't on this computer; the check never downloads");

    let tag = format!("{:x}", std::process::id());
    let name = format!("toolbox-check-{tag}");
    let volume = format!("toolbox-check-vol-{tag}");
    let spec = RunSpec {
        image,
        name: name.clone(),
        ports: vec![PortSpec { host: 18089, container: 3000, udp: false }],
        localhost_only: true,
        volumes: vec![VolumeSpec { source: volume.clone(), target: "/data".into(), read_only: false }],
        env: vec!["TOOLBOX_CHECK=1".into()],
        restart: "no".into(),
    };

    let result = run(&docker, &spec).await;
    let outcome = match &result {
        Ok(id) => {
            let details = engine::inspect(&docker, id).await.expect("inspect");
            println!("started {}", details.name);
            println!("ports: {:?}", details.ports);
            println!("mounts: {:?}", details.mounts);
            println!("env has the variable: {}", details.env.iter().any(|e| e == "TOOLBOX_CHECK=1"));
            assert_eq!(details.name, name);
            assert!(details.ports.iter().any(|p| p.contains("127.0.0.1:18089")), "port not on localhost only");
            assert!(details.mounts.iter().any(|m| m.contains("/data")));
            // A second container with the same name must be refused in plain words.
            let again = run(&docker, &spec).await.unwrap_err();
            println!("same name again: {}", again.message);
            Ok(())
        }
        Err(e) => Err(e.message.clone()),
    };

    // Clean up whatever happened.
    if let Ok(id) = &result {
        let _ = engine::act(&docker, id, Action::Stop).await;
        engine::act(&docker, id, Action::Remove).await.expect("remove the container");
    }
    let _ = resources::remove(&docker, Kind::Volumes, &volume).await;
    outcome.expect("run failed");
    println!("ok");
}
