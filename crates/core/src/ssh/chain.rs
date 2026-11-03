//! Connecting to a session, through its jump hosts if it has any (INTERFACES.md section 7,
//! "Jump hosts"). Each hop's host key is checked, and each hop logs in with its own saved method.

use super::auth::{Login, LoginMethod, login};
use super::backend::SshServices;
use super::connection::{Connection, connect, connect_through};
use crate::error::{AppError, ErrorCode};
use crate::prompt::Prompter;
use crate::sessions::{SavedSession, SessionsFile, SshAuth};

/// The most jump hosts one session may go through.
pub const MAX_JUMPS: usize = 3;

/// One machine on the way, from the first jump host to the session itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    /// The saved session, which names this hop's entries in the keychain.
    pub id: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub method: LoginMethod,
}

fn method_of(auth: SshAuth, key_path: &str) -> LoginMethod {
    match auth {
        SshAuth::Password => LoginMethod::Password,
        SshAuth::Key => LoginMethod::Key {
            path: key_path.to_string(),
        },
        SshAuth::Agent => LoginMethod::Agent,
    }
}

fn hop_of(session: &SavedSession) -> Option<(Hop, &str)> {
    match session {
        SavedSession::Ssh {
            id,
            host,
            port,
            user,
            auth,
            key_path,
            jump,
            ..
        } => Some((
            Hop {
                id: id.clone(),
                host: host.clone(),
                port: *port,
                user: user.clone(),
                method: method_of(*auth, key_path),
            },
            jump,
        )),
        _ => None,
    }
}

/// The machines to go through to reach `target_id`: its jump hosts in order, then the session.
pub fn resolve_chain(file: &SessionsFile, target_id: &str) -> Result<Vec<Hop>, AppError> {
    let not_found = || {
        AppError::new(
            ErrorCode::NotFound,
            "That saved session doesn't exist any more.",
        )
    };
    let find = |id: &str| file.sessions.iter().find(|s| s.id() == id);

    let mut chain = Vec::new();
    let mut at = target_id.to_string();
    loop {
        let session = find(&at).ok_or_else(not_found)?;
        let (hop, jump) = hop_of(session).ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                "A jump host must be an SSH session.",
            )
        })?;
        chain.push(hop);
        if jump.is_empty() {
            break;
        }
        if chain.len() > MAX_JUMPS {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("A session can go through at most {MAX_JUMPS} jump hosts."),
            ));
        }
        at = jump.to_string();
    }
    chain.reverse();
    Ok(chain)
}

/// Connects along the chain. Returns the last connection, and the earlier ones that must stay
/// open for it to keep working. `final_method` replaces the login of the last hop only.
pub async fn connect_chain(
    services: &SshServices,
    chain: &[Hop],
    final_method: Option<LoginMethod>,
    prompter: &Prompter,
) -> Result<(Connection, Vec<Connection>), AppError> {
    let mut earlier: Vec<Connection> = Vec::new();
    let mut last: Option<Connection> = None;
    for (i, hop) in chain.iter().enumerate() {
        // Each hop is reached through the one just before it.
        let mut connection = match last.as_ref() {
            None => connect(
                &hop.host,
                hop.port,
                services.known_hosts.clone(),
                prompter.clone(),
            )
            .await
            .map_err(|e| on_hop(e, hop, i, chain.len()))?,
            Some(through) => connect_through(
                through,
                &hop.host,
                hop.port,
                services.known_hosts.clone(),
                prompter.clone(),
            )
            .await
            .map_err(|e| on_hop(e, hop, i, chain.len()))?,
        };
        let is_last = i + 1 == chain.len();
        let method = match (&final_method, is_last) {
            (Some(m), true) => m.clone(),
            _ => hop.method.clone(),
        };
        login(
            &mut connection,
            &Login {
                session_id: hop.id.clone(),
                host: hop.host.clone(),
                user: hop.user.clone(),
                method,
            },
            &services.keychain,
            prompter,
        )
        .await
        .map_err(|e| on_hop(e, hop, i, chain.len()))?;
        if let Some(previous) = last.take() {
            earlier.push(previous);
        }
        last = Some(connection);
    }
    let last = last
        .ok_or_else(|| AppError::new(ErrorCode::InvalidInput, "There is nothing to connect to."))?;
    Ok((last, earlier))
}

/// Says which machine failed when there is more than one on the way. Only a message that
/// explains a failure is changed: questions and cancellations pass through.
fn on_hop(mut error: AppError, hop: &Hop, index: usize, total: usize) -> AppError {
    if total > 1 && error.code != ErrorCode::Cancelled {
        error.message = format!(
            "{} (step {} of {}, {})",
            error.message,
            index + 1,
            total,
            hop.host
        );
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh(id: &str, jump: &str) -> SavedSession {
        SavedSession::Ssh {
            id: id.into(),
            name: id.into(),
            folder: String::new(),
            host: format!("{id}.example"),
            port: 22,
            user: "u".into(),
            auth: SshAuth::Agent,
            key_path: String::new(),
            jump: jump.into(),
            x11: false,
        }
    }

    fn file(sessions: Vec<SavedSession>) -> SessionsFile {
        SessionsFile {
            version: 1,
            folders: vec![],
            sessions,
        }
    }

    #[test]
    fn no_jump_is_a_chain_of_one() {
        let chain = resolve_chain(&file(vec![ssh("a", "")]), "a").unwrap();
        assert_eq!(
            chain.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
    }

    #[test]
    fn jump_hosts_come_first_in_the_order_they_are_used() {
        let f = file(vec![ssh("target", "j2"), ssh("j2", "j1"), ssh("j1", "")]);
        let chain = resolve_chain(&f, "target").unwrap();
        assert_eq!(
            chain.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            ["j1", "j2", "target"]
        );
        assert_eq!(chain[2].host, "target.example");
    }

    #[test]
    fn three_jump_hosts_are_allowed_and_four_are_not() {
        let three = file(vec![
            ssh("t", "a"),
            ssh("a", "b"),
            ssh("b", "c"),
            ssh("c", ""),
        ]);
        assert_eq!(resolve_chain(&three, "t").unwrap().len(), 4);
        let four = file(vec![
            ssh("t", "a"),
            ssh("a", "b"),
            ssh("b", "c"),
            ssh("c", "d"),
            ssh("d", ""),
        ]);
        assert_eq!(
            resolve_chain(&four, "t").unwrap_err().code,
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn a_loop_or_a_missing_or_wrong_kind_of_jump_host_is_an_error() {
        let looped = file(vec![ssh("a", "b"), ssh("b", "a")]);
        assert!(resolve_chain(&looped, "a").is_err());
        let missing = file(vec![ssh("a", "ghost")]);
        assert_eq!(
            resolve_chain(&missing, "a").unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            resolve_chain(&missing, "nope").unwrap_err().code,
            ErrorCode::NotFound
        );
    }
}
