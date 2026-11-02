//! Images, volumes and networks, against the fake engine.

use bollard::Docker;

use super::fake::{Requests, fake_docker_engine};
use super::resources::{Kind, list, remove};
use crate::error::ErrorCode;

async fn engine() -> (Docker, Requests) {
    let (port, seen) = fake_docker_engine().await;
    let docker = Docker::connect_with_http(
        &format!("http://127.0.0.1:{port}"),
        10,
        bollard::API_DEFAULT_VERSION,
    )
    .unwrap();
    (docker, seen)
}

#[tokio::test]
async fn each_kind_is_listed_in_plain_words() {
    let (docker, _) = engine().await;
    let images = list(&docker, Kind::Images).await.unwrap();
    assert_eq!(images.len(), 2);
    let nginx = images.iter().find(|i| i.name == "nginx:latest").unwrap();
    assert_eq!(nginx.detail, "aabbccddeeff · 142.0 MB");
    assert!(images.iter().any(|i| i.name == "(untagged)"));

    let volumes = list(&docker, Kind::Volumes).await.unwrap();
    assert_eq!(volumes[0].name, "data");
    assert!(volumes[0].detail.starts_with("local"));

    let networks = list(&docker, Kind::Networks).await.unwrap();
    assert!(
        networks
            .iter()
            .find(|n| n.name == "bridge")
            .unwrap()
            .protected
    );
    assert!(
        !networks
            .iter()
            .find(|n| n.name == "shop_default")
            .unwrap()
            .protected
    );
}

#[tokio::test]
async fn removing_sends_one_request_and_never_forces() {
    let (docker, seen) = engine().await;
    remove(&docker, Kind::Images, "sha256:0123456789ab")
        .await
        .unwrap();
    remove(&docker, Kind::Volumes, "data").await.unwrap();
    remove(&docker, Kind::Networks, "bb22").await.unwrap();
    let seen = seen.lock().unwrap().join("\n");
    assert!(
        seen.contains("DELETE /images/sha256:0123456789ab"),
        "{seen}"
    );
    assert!(seen.contains("DELETE /volumes/data"));
    assert!(seen.contains("DELETE /networks/bb22"));
    assert!(!seen.contains("force=true"));
}

#[tokio::test]
async fn names_that_could_change_the_request_are_refused() {
    let (docker, seen) = engine().await;
    for (kind, bad) in [
        (Kind::Images, "nginx"),
        (Kind::Images, "../x"),
        (Kind::Volumes, "../x"),
        (Kind::Volumes, "a/b"),
        (Kind::Volumes, "a?force=true"),
        (Kind::Volumes, ""),
        (Kind::Networks, "bridge/x"),
    ] {
        let error = remove(&docker, kind, bad).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput, "{bad}");
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn details_come_as_plain_lines() {
    let (docker, _) = engine().await;
    let image = super::resources::inspect(&docker, Kind::Images, "sha256:aabbccddeeff00112233")
        .await
        .unwrap();
    let text = image.lines.join("\n");
    assert!(text.contains("Tags: nginx:latest"), "{text}");
    assert!(text.contains("System: linux/amd64"), "{text}");
    assert!(text.contains("Ports: 80/tcp"), "{text}");
    assert!(text.contains("Label maintainer: me"), "{text}");

    let network = super::resources::inspect(&docker, Kind::Networks, "bb22")
        .await
        .unwrap();
    let text = network.lines.join("\n");
    assert!(text.contains("Subnet: 172.18.0.0/16"), "{text}");
    assert!(text.contains("Container: web 172.18.0.2/16"), "{text}");

    let volume = super::resources::inspect(&docker, Kind::Volumes, "data")
        .await
        .unwrap();
    assert!(volume.lines.join("\n").contains("Mounted at: /var/lib/docker"));
}

#[tokio::test]
async fn clearing_out_counts_what_went() {
    use super::resources::{PruneKind, prune};
    let (docker, _) = engine().await;
    let p = prune(&docker, PruneKind::Containers).await.unwrap();
    assert_eq!((p.removed, p.bytes), (2, 2048));
    assert_eq!(prune(&docker, PruneKind::Images).await.unwrap().bytes, 5_000_000);
    assert_eq!(prune(&docker, PruneKind::Volumes).await.unwrap().removed, 1);
    assert_eq!(prune(&docker, PruneKind::Networks).await.unwrap().removed, 3);
}

#[tokio::test]
async fn pulling_waits_and_checks_the_name() {
    use super::resources::pull;
    let (docker, seen) = engine().await;
    pull(&docker, "nginx:1.27").await.unwrap();
    assert!(seen.lock().unwrap().join("\n").contains("fromImage=nginx"));
    for bad in ["", "-rm", "a b", "x?y=1", "a;b"] {
        assert_eq!(pull(&docker, bad).await.unwrap_err().code, ErrorCode::InvalidInput, "{bad}");
    }
}
