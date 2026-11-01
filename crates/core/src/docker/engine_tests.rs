//! The container screen's logic, against the fake engine.

use bollard::Docker;

use super::engine::{Action, act, check_id, clean, inspect, list_containers};
use super::fake::{Requests, fake_docker_engine};
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
async fn containers_come_newest_first_with_compose_details() {
    let (docker, _) = engine().await;
    let list = list_containers(&docker).await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "web");
    assert_eq!(list[0].state, "running");
    assert_eq!(list[0].project.as_deref(), Some("shop"));
    assert_eq!(list[0].service.as_deref(), Some("web"));
    assert_eq!(list[0].ports, vec!["0.0.0.0:8080->80/tcp"]);
    assert_eq!(list[1].project, None);
}

#[tokio::test]
async fn names_from_docker_lose_their_control_characters() {
    let (docker, _) = engine().await;
    let list = list_containers(&docker).await.unwrap();
    assert_eq!(list[1].name, "old[31m");
    assert_eq!(clean("a\u{7}b\n\u{1b}c"), "abc");
    assert!(clean(&"x".repeat(1000)).chars().count() <= 301);
}

#[tokio::test]
async fn each_action_sends_the_right_request() {
    let (docker, seen) = engine().await;
    act(&docker, "abc123", Action::Start).await.unwrap();
    act(&docker, "abc123", Action::Stop).await.unwrap();
    act(&docker, "abc123", Action::Restart).await.unwrap();
    act(&docker, "abc123", Action::Remove).await.unwrap();
    let seen = seen.lock().unwrap().join("\n");
    assert!(
        seen.contains("POST ") && seen.contains("/containers/abc123/start"),
        "{seen}"
    );
    assert!(seen.contains("/containers/abc123/stop"));
    assert!(seen.contains("/containers/abc123/restart"));
    assert!(seen.contains("DELETE "));
}

#[tokio::test]
async fn removing_never_forces_a_running_container() {
    let (docker, seen) = engine().await;
    act(&docker, "abc123", Action::Remove).await.unwrap();
    let seen = seen.lock().unwrap().join("\n");
    let delete = seen.lines().find(|l| l.starts_with("DELETE")).unwrap();
    assert!(!delete.contains("force=true"), "{delete}");
}

#[tokio::test]
async fn a_missing_container_is_reported_in_plain_words() {
    let (docker, _) = engine().await;
    let error = inspect(&docker, "fade").await.unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(error.message.contains("no longer exists"));
}

#[tokio::test]
async fn ids_that_could_change_the_request_are_refused() {
    let (docker, seen) = engine().await;
    for bad in [
        "",
        "../images",
        "abc/stop",
        "abc?force=true",
        "zz",
        &"a".repeat(65),
    ] {
        let error = act(&docker, bad, Action::Remove).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput, "{bad}");
        assert!(check_id(bad).is_err());
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn details_list_environment_ports_mounts_and_networks() {
    let (docker, _) = engine().await;
    let d = inspect(&docker, "abc123").await.unwrap();
    assert_eq!(d.name, "web");
    assert_eq!(d.state, "running");
    assert!(!d.tty);
    assert_eq!(d.env, vec!["MODE=test", "TOKEN=abc"]);
    assert_eq!(d.ports, vec!["0.0.0.0:8080 -> 80/tcp", "443/tcp"]);
    assert_eq!(
        d.mounts,
        vec!["/srv/site -> /usr/share/nginx/html (read-only)"]
    );
    assert_eq!(d.networks, vec!["shop_default (172.18.0.2)"]);
    assert!(d.raw.contains("\"Config\""));
}

#[test]
fn stats_are_read_like_docker_does() {
    let sample = serde_json::json!({
        "cpu_stats": {"cpu_usage": {"total_usage": 300}, "system_cpu_usage": 2000, "online_cpus": 4},
        "precpu_stats": {"cpu_usage": {"total_usage": 100}, "system_cpu_usage": 1000},
        "memory_stats": {"usage": 1000, "limit": 4000, "stats": {"inactive_file": 200}}
    });
    let s = super::engine::read_stats("abc", &sample).unwrap();
    assert!((s.cpu_percent - 80.0).abs() < 1e-9);
    assert_eq!((s.memory_used, s.memory_limit), (800, 4000));
    assert!(super::engine::read_stats("abc", &serde_json::json!({})).is_none());
}
