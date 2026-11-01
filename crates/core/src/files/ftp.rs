//! Files on an FTP or FTPS server (F5, SEC6, SEC8).
//!
//! Plain FTP sends everything, passwords included, without protection, so it is only used after
//! the user has agreed to that, every time. For FTPS the server's certificate is checked like
//! any other. If the check fails, the user is shown why and the certificate's fingerprint, and the
//! connection goes ahead only if they accept it. The accepted fingerprint is then trusted for
//! that session, until the server presents a different one.

use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use async_trait::async_trait;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use sha2::{Digest, Sha256};
use suppaftp::list::{File as ListedFile, ListParser};
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::types::FileType as TransferType;
use suppaftp::{FtpError, Status};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use super::backend::{FileBackend, FileCapabilities, FileEntry, FileKind, Progress};
use super::local::copy_bytes;
use crate::error::{AppError, ErrorCode};
use crate::keychain::{Keychain, SecretKind};
use crate::prompt::{Prompt, Prompter};
use crate::sessions::FtpSecurity;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Where to connect, and as whom.
pub struct FtpTarget {
    /// The saved session, which names the password in the keychain.
    pub session_id: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub security: FtpSecurity,
    /// The fingerprint the user accepted before, or empty.
    pub trusted_cert_sha256: String,
}

/// What a successful connection leaves for the caller.
pub struct FtpOpened {
    pub files: FtpFiles,
    /// A certificate the user accepted just now. The caller saves it with the session.
    pub accepted_cert: Option<String>,
}

/// Why a certificate wasn't accepted, as shown to the user.
#[derive(Debug, Clone)]
struct CertProblem {
    fingerprint: String,
    reason: String,
    subject: String,
    expires: String,
}

type Problem = Arc<Mutex<Option<CertProblem>>>;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn fingerprint_of(cert: &CertificateDer<'_>) -> String {
    hex(&Sha256::digest(cert.as_ref()))
}

/// Accepts a certificate that passes the normal checks, or that is exactly the one the user
/// accepted. Anything else is refused and remembered, so the user can be asked about it.
#[derive(Debug)]
struct PinVerifier {
    pinned: String,
    /// `None` when this computer has no trusted authorities at all.
    inner: Option<Arc<WebPkiServerVerifier>>,
    algorithms: WebPkiSupportedAlgorithms,
    problem: Problem,
}

fn reason_for(error: &rustls::Error, pinned: bool) -> String {
    let base = match error {
        rustls::Error::InvalidCertificate(CertificateError::Expired) => {
            "The certificate has expired."
        }
        rustls::Error::InvalidCertificate(CertificateError::NotValidYet) => {
            "The certificate isn't valid yet."
        }
        rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer) => {
            "The certificate wasn't issued by an authority this computer trusts. It may be self-signed."
        }
        rustls::Error::InvalidCertificate(
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
        ) => "The certificate isn't for this server's name.",
        _ => "The certificate couldn't be verified.",
    };
    if pinned {
        format!("It is not the certificate you accepted before. {base}")
    } else {
        base.to_string()
    }
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fingerprint = fingerprint_of(end_entity);
        if !self.pinned.is_empty() && self.pinned.eq_ignore_ascii_case(&fingerprint) {
            return Ok(ServerCertVerified::assertion());
        }
        let checked = match &self.inner {
            Some(inner) => {
                inner.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
            }
            None => Err(rustls::Error::InvalidCertificate(
                CertificateError::UnknownIssuer,
            )),
        };
        if let Err(error) = &checked {
            let (subject, expires) = x509_parser::parse_x509_certificate(end_entity.as_ref())
                .map(|(_, c)| (c.subject().to_string(), c.validity().not_after.to_string()))
                .unwrap_or_default();
            *self.problem.lock().unwrap() = Some(CertProblem {
                fingerprint,
                reason: reason_for(error, !self.pinned.is_empty()),
                subject,
                expires,
            });
        }
        checked
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// The trusted authorities of this computer.
fn system_roots() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    roots
}

fn tls_connector(
    pinned: &str,
    roots: RootCertStore,
    problem: Problem,
) -> Result<AsyncRustlsConnector, AppError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .ok();
    let verifier = PinVerifier {
        pinned: pinned.to_string(),
        inner,
        algorithms: provider.signature_verification_algorithms,
        problem,
    };
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't set up a secure connection."))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(AsyncRustlsConnector::from(
        tokio_rustls::TlsConnector::from(Arc::new(config)),
    ))
}

/// Turns what FTP and the library report into a plain message. The server's own wording isn't
/// shown as it is, because it comes from the server.
fn from_ftp(error: &FtpError, path: &str) -> AppError {
    match error {
        FtpError::UnexpectedResponse(response) => match response.status.code() {
            530 => AppError::new(
                ErrorCode::AuthFailed,
                "The server refused that user name or password.",
            ),
            550 => AppError::new(
                ErrorCode::NotFound,
                format!("{path} wasn't found, or you don't have permission to use it."),
            ),
            421 => lost(),
            553 => AppError::new(
                ErrorCode::PermissionDenied,
                format!("You don't have permission to use {path}."),
            ),
            _ => AppError::new(
                ErrorCode::Internal,
                format!("The server couldn't do that with {path}."),
            ),
        },
        FtpError::ConnectionError(_) | FtpError::BadResponse => lost(),
        FtpError::SecureError(_) => AppError::new(
            ErrorCode::ConnectionFailed,
            "The secure connection to the server failed.",
        ),
        _ => AppError::new(
            ErrorCode::Internal,
            format!("The server couldn't do that with {path}."),
        ),
    }
}

fn lost() -> AppError {
    AppError::new(
        ErrorCode::ConnectionLost,
        "The connection to the server was lost.",
    )
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn entry_of(dir: &str, file: &ListedFile) -> FileEntry {
    let kind = if file.is_directory() {
        FileKind::Dir
    } else if file.is_symlink() {
        FileKind::Symlink
    } else if file.is_file() {
        FileKind::File
    } else {
        FileKind::Other
    };
    let modified = file
        .modified()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok());
    FileEntry {
        name: file.name().to_string(),
        path: join(dir, file.name()),
        kind,
        size: (kind == FileKind::File).then(|| file.size() as u64),
        modified,
        permissions: None,
    }
}

pub struct FtpFiles {
    /// One command at a time: FTP has a single control connection.
    control: tokio::sync::Mutex<AsyncRustlsFtpStream>,
}

async fn open_stream(
    target: &FtpTarget,
    connector: Option<AsyncRustlsConnector>,
) -> Result<AsyncRustlsFtpStream, FtpError> {
    let address = (target.host.as_str(), target.port);
    let attempt = async {
        match (target.security, connector) {
            (FtpSecurity::Implicit, Some(connector)) => {
                AsyncRustlsFtpStream::connect_secure_implicit(address, connector, &target.host)
                    .await
            }
            (FtpSecurity::Explicit, Some(connector)) => {
                AsyncRustlsFtpStream::connect(address)
                    .await?
                    .into_secure(connector, &target.host)
                    .await
            }
            _ => AsyncRustlsFtpStream::connect(address).await,
        }
    };
    tokio::time::timeout(CONNECT_TIMEOUT, attempt)
        .await
        .unwrap_or_else(|_| {
            Err(FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "timed out",
            )))
        })
}

impl FtpFiles {
    /// Connects, asking the user whatever is needed on the way: the warning for plain FTP, a
    /// certificate that doesn't check out, and the password.
    pub async fn connect(
        target: &FtpTarget,
        keychain: &Keychain,
        prompter: &Prompter,
    ) -> Result<FtpOpened, AppError> {
        Self::connect_with_roots(target, keychain, prompter, system_roots()).await
    }

    pub(crate) async fn connect_with_roots(
        target: &FtpTarget,
        keychain: &Keychain,
        prompter: &Prompter,
        roots: RootCertStore,
    ) -> Result<FtpOpened, AppError> {
        let declined = || AppError::new(ErrorCode::Cancelled, "The connection was cancelled.");

        let mut accepted_cert = None;
        let mut stream = if target.security == FtpSecurity::Plain {
            // SEC6: said every time, because this sends the password in the open.
            let answer = prompter
                .ask(Prompt::PlainFtp {
                    host: target.host.clone(),
                })
                .await?;
            if !answer.accepted {
                return Err(declined());
            }
            open_stream(target, None)
                .await
                .map_err(|e| from_ftp(&e, &target.host))
                .map_err(connection_failed)?
        } else {
            let mut pinned = target.trusted_cert_sha256.clone();
            loop {
                let problem: Problem = Arc::default();
                let connector = tls_connector(&pinned, roots.clone(), problem.clone())?;
                match open_stream(target, Some(connector)).await {
                    Ok(stream) => break stream,
                    Err(error) => {
                        let Some(found) = problem.lock().unwrap().take() else {
                            return Err(connection_failed(from_ftp(&error, &target.host)));
                        };
                        // SEC8: the user decides, with the reason and the fingerprint in front
                        // of them. Nothing is accepted without that.
                        let answer = prompter
                            .ask(Prompt::FtpsCertificate {
                                host: target.host.clone(),
                                reason: found.reason,
                                fingerprint: found.fingerprint.clone(),
                                subject: found.subject,
                                expires: found.expires,
                            })
                            .await?;
                        if !answer.accepted {
                            return Err(AppError::new(
                                ErrorCode::CertificateRejected,
                                "The server's certificate was not accepted.",
                            ));
                        }
                        accepted_cert = Some(found.fingerprint.clone());
                        pinned = found.fingerprint;
                    }
                }
            }
        };

        Self::log_in(&mut stream, target, keychain, prompter).await?;
        if target.security == FtpSecurity::Implicit {
            // Explicit TLS sets this up itself. With implicit TLS the data connections would
            // otherwise be left unprotected, or refused.
            for command in ["PBSZ 0", "PROT P"] {
                stream
                    .custom_command(command, &[Status::CommandOk])
                    .await
                    .map_err(|e| from_ftp(&e, &target.host))?;
            }
        }
        // Passive mode: the server says where to open the data connection. A hostile server
        // could name some other computer, so the address of the control connection is used
        // instead, and only the port from the server's answer.
        stream.set_passive_nat_workaround(true);
        stream
            .transfer_type(TransferType::Binary)
            .await
            .map_err(|e| from_ftp(&e, &target.host))?;
        Ok(FtpOpened {
            files: FtpFiles {
                control: tokio::sync::Mutex::new(stream),
            },
            accepted_cert,
        })
    }

    async fn log_in(
        stream: &mut AsyncRustlsFtpStream,
        target: &FtpTarget,
        keychain: &Keychain,
        prompter: &Prompter,
    ) -> Result<(), AppError> {
        let refused = |e: &FtpError| from_ftp(e, &target.host);
        // No user name means an anonymous login, which has no password to ask for.
        if target.user.is_empty() {
            return stream
                .login("anonymous", "anonymous@")
                .await
                .map_err(|e| refused(&e));
        }
        if let Ok(Some(saved)) = keychain
            .get(&target.session_id, SecretKind::FtpPassword)
            .await
            && stream
                .login(target.user.as_str(), saved.expose())
                .await
                .is_ok()
        {
            return Ok(());
        }
        let answer = prompter
            .ask(Prompt::Password {
                user: target.user.clone(),
                host: target.host.clone(),
                offer_save: keychain.is_available(),
            })
            .await?;
        let Some(typed) = answer.text.filter(|_| answer.accepted) else {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "The login was cancelled.",
            ));
        };
        stream
            .login(target.user.as_str(), typed.expose())
            .await
            .map_err(|e| refused(&e))?;
        if answer.save {
            let _ = keychain
                .set(&target.session_id, SecretKind::FtpPassword, typed)
                .await;
        }
        Ok(())
    }

    /// The folder a session starts in.
    pub async fn home(&self) -> Result<String, AppError> {
        self.control
            .lock()
            .await
            .pwd()
            .await
            .map_err(|e| from_ftp(&e, "."))
    }
}

fn connection_failed(error: AppError) -> AppError {
    if error.code == ErrorCode::ConnectionLost {
        AppError::new(
            ErrorCode::ConnectionFailed,
            "Couldn't connect to the server.",
        )
    } else {
        error
    }
}

/// Reads a listing line, in whichever of the listing styles servers use.
fn parse_line(line: &str, machine: bool) -> Option<ListedFile> {
    let parsed = if machine {
        ListParser::parse_mlsd(line).ok()
    } else {
        ListedFile::try_from(line).ok()
    };
    parsed.filter(|f| f.name() != "." && f.name() != "..")
}

#[async_trait]
impl FileBackend for FtpFiles {
    fn capabilities(&self) -> FileCapabilities {
        FileCapabilities { chmod: false }
    }

    async fn list(&self, path: &str) -> Result<Vec<FileEntry>, AppError> {
        let mut ftp = self.control.lock().await;
        // MLSD is the exact way. Older servers only have LIST.
        let (lines, machine) = match ftp.mlsd(Some(path)).await {
            Ok(lines) => (lines, true),
            Err(FtpError::UnexpectedResponse(r)) if matches!(r.status.code(), 500..=504) => (
                ftp.list(Some(path)).await.map_err(|e| from_ftp(&e, path))?,
                false,
            ),
            Err(e) => return Err(from_ftp(&e, path)),
        };
        Ok(lines
            .iter()
            .filter_map(|line| parse_line(line, machine))
            .map(|f| entry_of(path, &f))
            .collect())
    }

    async fn stat(&self, path: &str) -> Result<FileEntry, AppError> {
        let trimmed = path.trim_end_matches('/');
        let (dir, name) = match trimmed.rsplit_once('/') {
            Some(("", name)) => ("/", name),
            Some((dir, name)) => (dir, name),
            None => (".", trimmed),
        };
        if name.is_empty() {
            // The top folder has nothing above it to list it from.
            return Ok(FileEntry {
                name: "/".into(),
                path: "/".into(),
                kind: FileKind::Dir,
                size: None,
                modified: None,
                permissions: None,
            });
        }
        self.list(dir)
            .await?
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("{path} wasn't found.")))
    }

    async fn mkdir(&self, path: &str) -> Result<(), AppError> {
        let made = self.control.lock().await.mkdir(path).await;
        match made {
            Ok(()) => Ok(()),
            Err(error) => {
                if self.stat(path).await.is_ok() {
                    return Err(AppError::new(
                        ErrorCode::AlreadyExists,
                        format!("{path} already exists."),
                    ));
                }
                Err(from_ftp(&error, path))
            }
        }
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), AppError> {
        self.control
            .lock()
            .await
            .rename(from, to)
            .await
            .map_err(|e| from_ftp(&e, from))
    }

    async fn remove_file(&self, path: &str) -> Result<(), AppError> {
        self.control
            .lock()
            .await
            .rm(path)
            .await
            .map_err(|e| from_ftp(&e, path))
    }

    async fn remove_dir(&self, path: &str) -> Result<(), AppError> {
        self.control
            .lock()
            .await
            .rmdir(path)
            .await
            .map_err(|e| from_ftp(&e, path))
    }

    async fn chmod(&self, _path: &str, _mode: u32) -> Result<(), AppError> {
        Err(AppError::new(
            ErrorCode::Unsupported,
            "FTP servers don't all allow changing permissions.",
        ))
    }

    async fn read_to(
        &self,
        path: &str,
        dest: &mut (dyn AsyncWrite + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError> {
        let mut ftp = self.control.lock().await;
        let mut data = ftp
            .retr_as_stream(path)
            .await
            .map_err(|e| from_ftp(&e, path))?;
        // `copy_bytes` ends the destination too, which is what the part file needs.
        match copy_bytes(&mut data, dest, progress, &cancel, path).await {
            Ok(()) => data.finish().await.map_err(|e| from_ftp(&e, path)),
            Err(error) => {
                let _ = ftp.abort(data).await;
                Err(error)
            }
        }
    }

    async fn write_from(
        &self,
        path: &str,
        src: &mut (dyn AsyncRead + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError> {
        let mut ftp = self.control.lock().await;
        let mut data = ftp
            .put_with_stream(path)
            .await
            .map_err(|e| from_ftp(&e, path))?;
        match copy_bytes(src, &mut data, progress, &cancel, path).await {
            Ok(()) => data.finish().await.map_err(|e| from_ftp(&e, path)),
            Err(error) => {
                let _ = ftp.abort(data).await;
                Err(error)
            }
        }
    }
}
