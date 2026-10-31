//! Library check: keyring-core with the platform store.
//! Windows: set, read and delete a test entry, then confirm it is gone.
//! Linux: builds only (no Secret Service is assumed to be running).

#[cfg(windows)]
fn main() {
    use keyring_core::{Entry, Error};

    keyring_core::set_default_store(windows_native_keyring_store::Store::new().expect("store"));
    let entry = Entry::new("SSHToolbox-test", "session:test:password").expect("entry");
    entry.set_password("not-a-real-password").expect("set");
    assert_eq!(entry.get_password().expect("get"), "not-a-real-password");
    entry.delete_credential().expect("delete");
    assert!(
        matches!(entry.get_password(), Err(Error::NoEntry)),
        "entry should be gone after delete"
    );
    println!("keyring-core + windows-native-keyring-store OK: set, read, delete, confirmed gone");
}

#[cfg(target_os = "linux")]
fn main() {
    // Only constructs the type: a real Secret Service is tested on the owner's Ubuntu machine.
    let _build_only: fn() -> keyring_core::Result<_> =
        zbus_secret_service_keyring_store::Store::new;
    println!("keyring-core + zbus-secret-service-keyring-store: builds");
}

#[cfg(not(any(windows, target_os = "linux")))]
fn main() {}
