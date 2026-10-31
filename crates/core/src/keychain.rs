//! Saved passwords and passphrases, kept in the system keychain (INTERFACES.md section 10).
//!
//! The keychain can fail and it can hang (the Ubuntu test machine has shown both), so every call
//! runs on a blocking thread with a time limit. After a failure or a timeout the keychain counts
//! as unavailable until the app restarts, and the rest of the app carries on by asking for
//! passwords each time. Secrets are never written anywhere else.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::error::{AppError, ErrorCode};
use crate::secret::Secret;

pub const SERVICE: &str = "SSHToolbox";
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// What a stored secret is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Password,
    Passphrase,
    FtpPassword,
}

impl SecretKind {
    const ALL: [SecretKind; 3] = [Self::Password, Self::Passphrase, Self::FtpPassword];

    fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Passphrase => "passphrase",
            Self::FtpPassword => "ftp_password",
        }
    }
}

/// The entry name for a session's secret, for example `session:1234:password`.
pub fn account_name(session_id: &str, kind: SecretKind) -> String {
    format!("session:{session_id}:{}", kind.as_str())
}

/// A place that keeps secrets. The real one is the system keychain; tests use a fake.
pub trait SecretStore: Send + Sync + 'static {
    fn set(&self, account: &str, secret: &Secret) -> Result<(), String>;
    /// `Ok(None)` means nothing is stored, which is not a failure.
    fn get(&self, account: &str) -> Result<Option<Secret>, String>;
    /// Deleting something that isn't there is not a failure.
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// The system keychain through `keyring-core`.
struct SystemStore;

impl SecretStore for SystemStore {
    fn set(&self, account: &str, secret: &Secret) -> Result<(), String> {
        keyring_core::Entry::new(SERVICE, account)
            .and_then(|e| e.set_password(secret.expose()))
            .map_err(|e| e.to_string())
    }

    fn get(&self, account: &str) -> Result<Option<Secret>, String> {
        let entry = keyring_core::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let entry = keyring_core::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn unavailable() -> AppError {
    AppError::new(
        ErrorCode::KeychainUnavailable,
        "The system keychain isn't available, so passwords will be asked for each time.",
    )
}

pub struct Keychain {
    store: Option<Arc<dyn SecretStore>>,
    broken: Arc<AtomicBool>,
    timeout: Duration,
}

impl Keychain {
    /// Connects to the system keychain. If that can't be done, the keychain is unavailable.
    pub fn system() -> Self {
        Self::with_store(connect_system(), TIMEOUT)
    }

    /// For tests and for systems without a keychain: `None` means unavailable.
    pub fn with_store(store: Option<Arc<dyn SecretStore>>, timeout: Duration) -> Self {
        Self {
            store,
            broken: Arc::new(AtomicBool::new(false)),
            timeout,
        }
    }

    pub fn is_available(&self) -> bool {
        self.store.is_some() && !self.broken.load(Ordering::SeqCst)
    }

    /// Runs one keychain call off the main work, with a time limit. A failure or a timeout turns
    /// the keychain off until restart.
    async fn run<T, F>(&self, call: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(Arc<dyn SecretStore>) -> Result<T, String> + Send + 'static,
    {
        let Some(store) = self.store.clone().filter(|_| self.is_available()) else {
            return Err(unavailable());
        };
        let task = tokio::task::spawn_blocking(move || call(store));
        match tokio::time::timeout(self.timeout, task).await {
            Ok(Ok(Ok(value))) => Ok(value),
            _ => {
                self.broken.store(true, Ordering::SeqCst);
                Err(unavailable())
            }
        }
    }

    pub async fn set(
        &self,
        session_id: &str,
        kind: SecretKind,
        secret: Secret,
    ) -> Result<(), AppError> {
        let account = account_name(session_id, kind);
        self.run(move |store| store.set(&account, &secret)).await
    }

    pub async fn get(
        &self,
        session_id: &str,
        kind: SecretKind,
    ) -> Result<Option<Secret>, AppError> {
        let account = account_name(session_id, kind);
        self.run(move |store| store.get(&account)).await
    }

    pub async fn delete(&self, session_id: &str, kind: SecretKind) -> Result<(), AppError> {
        let account = account_name(session_id, kind);
        self.run(move |store| store.delete(&account)).await
    }

    /// Deleting a session deletes all of its secrets.
    pub async fn delete_all(&self, session_id: &str) -> Result<(), AppError> {
        let session_id = session_id.to_string();
        self.run(move |store| {
            for kind in SecretKind::ALL {
                store.delete(&account_name(&session_id, kind))?;
            }
            Ok(())
        })
        .await
    }
}

#[cfg(windows)]
fn connect_system() -> Option<Arc<dyn SecretStore>> {
    let store = windows_native_keyring_store::Store::new().ok()?;
    keyring_core::set_default_store(store);
    Some(Arc::new(SystemStore))
}

#[cfg(target_os = "linux")]
fn connect_system() -> Option<Arc<dyn SecretStore>> {
    let store = zbus_secret_service_keyring_store::Store::new().ok()?;
    keyring_core::set_default_store(store);
    Some(Arc::new(SystemStore))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn connect_system() -> Option<Arc<dyn SecretStore>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A keychain in memory that can be told to fail or to hang.
    #[derive(Default)]
    struct Fake {
        items: Mutex<HashMap<String, String>>,
        fail: AtomicBool,
        hang: AtomicBool,
    }

    impl Fake {
        fn check(&self) -> Result<(), String> {
            if self.hang.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(600));
            }
            if self.fail.load(Ordering::SeqCst) {
                return Err("the keychain service is not running".into());
            }
            Ok(())
        }
    }

    impl SecretStore for Fake {
        fn set(&self, account: &str, secret: &Secret) -> Result<(), String> {
            self.check()?;
            self.items
                .lock()
                .unwrap()
                .insert(account.into(), secret.expose().into());
            Ok(())
        }
        fn get(&self, account: &str) -> Result<Option<Secret>, String> {
            self.check()?;
            Ok(self.items.lock().unwrap().get(account).map(Secret::new))
        }
        fn delete(&self, account: &str) -> Result<(), String> {
            self.check()?;
            self.items.lock().unwrap().remove(account);
            Ok(())
        }
    }

    fn keychain(fake: &Arc<Fake>) -> Keychain {
        let store: Arc<dyn SecretStore> = fake.clone();
        Keychain::with_store(Some(store), Duration::from_millis(100))
    }

    #[tokio::test]
    async fn stores_reads_and_deletes_a_secret() {
        let fake = Arc::new(Fake::default());
        let kc = keychain(&fake);
        assert!(kc.is_available());
        assert!(kc.get("s1", SecretKind::Password).await.unwrap().is_none());

        kc.set("s1", SecretKind::Password, Secret::new("pw-1"))
            .await
            .unwrap();
        let got = kc.get("s1", SecretKind::Password).await.unwrap().unwrap();
        assert_eq!(got.expose(), "pw-1");
        assert!(
            kc.get("s1", SecretKind::Passphrase)
                .await
                .unwrap()
                .is_none()
        );

        kc.delete("s1", SecretKind::Password).await.unwrap();
        assert!(kc.get("s1", SecretKind::Password).await.unwrap().is_none());
        kc.delete("s1", SecretKind::Password).await.unwrap(); // already gone: fine
        assert!(kc.is_available());
    }

    #[tokio::test]
    async fn entries_use_the_documented_names() {
        let fake = Arc::new(Fake::default());
        let kc = keychain(&fake);
        kc.set("abc", SecretKind::Passphrase, Secret::new("x"))
            .await
            .unwrap();
        kc.set("abc", SecretKind::FtpPassword, Secret::new("y"))
            .await
            .unwrap();
        let items = fake.items.lock().unwrap();
        assert!(items.contains_key("session:abc:passphrase"));
        assert!(items.contains_key("session:abc:ftp_password"));
    }

    #[tokio::test]
    async fn deleting_a_session_deletes_all_its_secrets_and_only_its_own() {
        let fake = Arc::new(Fake::default());
        let kc = keychain(&fake);
        for kind in SecretKind::ALL {
            kc.set("gone", kind, Secret::new("a")).await.unwrap();
        }
        kc.set("kept", SecretKind::Password, Secret::new("b"))
            .await
            .unwrap();
        kc.delete_all("gone").await.unwrap();
        let items = fake.items.lock().unwrap();
        assert_eq!(items.len(), 1);
        assert!(items.contains_key("session:kept:password"));
    }

    #[tokio::test]
    async fn a_failing_keychain_turns_off_and_stays_off() {
        let fake = Arc::new(Fake::default());
        let kc = keychain(&fake);
        fake.fail.store(true, Ordering::SeqCst);

        let err = kc
            .set("s1", SecretKind::Password, Secret::new("pw"))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::KeychainUnavailable);
        assert!(!kc.is_available());

        // Even if the service comes back, nothing is tried again until the app restarts.
        fake.fail.store(false, Ordering::SeqCst);
        let err = kc.get("s1", SecretKind::Password).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::KeychainUnavailable);
        assert!(fake.items.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_hanging_keychain_is_given_up_on_after_the_time_limit() {
        let fake = Arc::new(Fake::default());
        let kc = keychain(&fake);
        fake.hang.store(true, Ordering::SeqCst);

        let started = std::time::Instant::now();
        let err = kc.get("s1", SecretKind::Password).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::KeychainUnavailable);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "waited {:?}",
            started.elapsed()
        );
        assert!(!kc.is_available());
    }

    #[tokio::test]
    async fn without_a_store_everything_says_unavailable_without_trying() {
        let kc = Keychain::with_store(None, TIMEOUT);
        assert!(!kc.is_available());
        for result in [
            kc.get("s", SecretKind::Password).await.map(|_| ()),
            kc.delete("s", SecretKind::Password).await,
            kc.delete_all("s").await,
            kc.set("s", SecretKind::Password, Secret::new("x")).await,
        ] {
            assert_eq!(result.unwrap_err().code, ErrorCode::KeychainUnavailable);
        }
    }

    #[test]
    fn the_account_name_is_documented() {
        assert_eq!(
            account_name("1234", SecretKind::Password),
            "session:1234:password"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn the_real_windows_keychain_round_trips() {
        let kc = Keychain::system();
        if !kc.is_available() {
            return;
        }
        let id = format!("test-{}", uuid::Uuid::new_v4());
        kc.set(
            &id,
            SecretKind::Password,
            Secret::new("not-a-real-password"),
        )
        .await
        .unwrap();
        let got = kc.get(&id, SecretKind::Password).await.unwrap().unwrap();
        assert_eq!(got.expose(), "not-a-real-password");
        kc.delete_all(&id).await.unwrap();
        assert!(kc.get(&id, SecretKind::Password).await.unwrap().is_none());
    }
}
