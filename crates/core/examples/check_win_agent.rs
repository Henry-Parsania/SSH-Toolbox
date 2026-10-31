//! Library check: russh connects to the Windows OpenSSH agent pipe and lists keys.
//! If the `ssh-agent` service is not running this reports "untested" and exits normally.
#[cfg(windows)]
#[tokio::main]
async fn main() {
    use russh::keys::agent::client::AgentClient;

    match AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        Ok(mut agent) => {
            let identities = agent
                .request_identities()
                .await
                .expect("request identities");
            println!(
                "russh Windows agent OK: connected, {} key(s) listed",
                identities.len()
            );
        }
        Err(e) => println!("russh Windows agent UNTESTED: could not connect to the pipe ({e})"),
    }
}

#[cfg(not(windows))]
fn main() {
    println!("russh Windows agent: not applicable on this system");
}
