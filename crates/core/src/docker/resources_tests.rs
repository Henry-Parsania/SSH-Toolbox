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
    assert!(
        volume
            .lines
            .join("\n")
            .contains("Mounted at: /var/lib/docker")
    );
}

#[tokio::test]
async fn clearing_out_counts_what_went() {
    use super::resources::{PruneKind, prune};
    let (docker, _) = engine().await;
    let p = prune(&docker, PruneKind::Containers).await.unwrap();
    assert_eq!((p.removed, p.bytes), (2, 2048));
    assert_eq!(
        prune(&docker, PruneKind::Images).await.unwrap().bytes,
        5_000_000
    );
    assert_eq!(prune(&docker, PruneKind::Volumes).await.unwrap().removed, 1);
    assert_eq!(
        prune(&docker, PruneKind::Networks).await.unwrap().removed,
        3
    );
}

#[tokio::test]
async fn pulling_waits_and_checks_the_name() {
    use super::resources::pull;
    let (docker, seen) = engine().await;
    pull(&docker, "nginx:1.27", |_| {}).await.unwrap();
    assert!(seen.lock().unwrap().join("\n").contains("fromImage=nginx"));
    for bad in ["", "-rm", "a b", "x?y=1", "a;b"] {
        assert_eq!(
            pull(&docker, bad, |_| {}).await.unwrap_err().code,
            ErrorCode::InvalidInput,
            "{bad}"
        );
    }
}

#[tokio::test]
async fn pulling_reports_how_far_it_has_got_and_ends_complete() {
    use super::resources::pull;
    use std::sync::Mutex;
    let (docker, _) = engine().await;
    let seen = Mutex::new(Vec::new());
    pull(&docker, "nginx", |p| seen.lock().unwrap().push(p))
        .await
        .unwrap();
    let seen = seen.into_inner().unwrap();
    let last = seen.last().expect("at least the final report");
    assert_eq!((last.done, last.total), (100, 100));
}

#[tokio::test]
async fn lists_say_which_containers_use_what_and_how_big_volumes_are() {
    let (docker, _) = engine().await;
    let images = list(&docker, Kind::Images).await.unwrap();
    let nginx = images.iter().find(|i| i.name == "nginx:latest").unwrap();
    assert_eq!(nginx.used_by, vec!["web"]);
    assert_eq!(nginx.size, Some(142_000_000));
    let other = images.iter().find(|i| i.name == "(untagged)").unwrap();
    assert!(other.used_by.is_empty());

    let volumes = list(&docker, Kind::Volumes).await.unwrap();
    assert_eq!(volumes[0].used_by, vec!["web"]);
    assert_eq!(volumes[0].size, Some(2_048_000));
    assert_eq!(volumes[0].project.as_deref(), Some("shop"));

    let networks = list(&docker, Kind::Networks).await.unwrap();
    let shop = networks.iter().find(|n| n.name == "shop_default").unwrap();
    assert_eq!(shop.used_by, vec!["web"]);
    assert!(
        networks
            .iter()
            .find(|n| n.name == "bridge")
            .unwrap()
            .used_by
            .is_empty()
    );
}

#[tokio::test]
async fn a_volume_s_details_include_who_uses_it() {
    let (docker, _) = engine().await;
    let d = super::resources::inspect(&docker, Kind::Volumes, "data")
        .await
        .unwrap();
    let text = d.lines.join("\n");
    assert!(text.contains("In use by: web"), "{text}");
    assert!(text.contains("Size: 2.0 MB"), "{text}");
}

#[tokio::test]
async fn volumes_and_networks_can_be_made_with_checked_names() {
    use super::resources::create;
    let (docker, seen) = engine().await;
    create(&docker, Kind::Volumes, "backup-1").await.unwrap();
    create(&docker, Kind::Networks, "shop_net").await.unwrap();
    for bad in ["", "a/b", "../x", "-x", "a b"] {
        assert_eq!(
            create(&docker, Kind::Volumes, bad).await.unwrap_err().code,
            ErrorCode::InvalidInput,
            "{bad}"
        );
    }
    assert_eq!(
        create(&docker, Kind::Images, "x").await.unwrap_err().code,
        ErrorCode::InvalidInput
    );
    let seen = seen.lock().unwrap().join("\n");
    assert!(seen.contains("POST /volumes/create"), "{seen}");
    assert!(seen.contains("/networks/create"), "{seen}");
}
