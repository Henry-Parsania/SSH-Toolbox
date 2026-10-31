//! Library check: russh host key behavior matches the table in INTERFACES.md section 7.
//! Uses a temporary known_hosts file. The keys are public halves of throwaway keys.
use russh::keys::known_hosts::{
    check_known_hosts_path, known_host_keys_path, learn_known_hosts_path,
};
use russh::keys::{Error, parse_public_key_base64};

const ED_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIKgqmK9ropguFO7dbtvnHkHDGg1e+OzTvNO7hWlcj5yf";
const ED_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIFD7C7zMBtOyZdIBLxajc17aZb7tI2Wqz3jw0uDu+gvo";
const ECDSA: &str = "AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBGnaa8PYp9RS/n5ZoYO9Cwo9OFOdo3kAXbeuOc2PmnB4lv/DWGX61XoFcZTyvRGi39kPx133ETu3IsZLyMxLfhw=";

fn main() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("known_hosts");
    let ed_a = parse_public_key_base64(ED_A).expect("key a");
    let ed_b = parse_public_key_base64(ED_B).expect("key b");
    let ecdsa = parse_public_key_base64(ECDSA).expect("ecdsa");

    // Row: host not in the file (the file does not even exist yet) -> no keys, check says false.
    assert!(
        known_host_keys_path("example.test", 22, &path)
            .map(|k| k.is_empty())
            .unwrap_or(true)
    );
    println!("unknown host, no file:        no keys for host");

    learn_known_hosts_path("example.test", 22, &ed_a, &path).expect("learn");

    // Row: matching key -> Ok(true).
    assert!(matches!(
        check_known_hosts_path("example.test", 22, &ed_a, &path),
        Ok(true)
    ));
    println!("same key:                     Ok(true)");

    // Row: unknown host while the file has other hosts -> Ok(false) and no keys for this host.
    assert!(matches!(
        check_known_hosts_path("other.test", 22, &ed_a, &path),
        Ok(false)
    ));
    assert!(
        known_host_keys_path("other.test", 22, &path)
            .unwrap()
            .is_empty()
    );
    println!("unknown host, file has others: Ok(false), no keys for host");

    // Row: same type, different key -> Err(KeyChanged { line }).
    match check_known_hosts_path("example.test", 22, &ed_b, &path) {
        Err(Error::KeyChanged { line }) => {
            println!("changed key, same type:       KeyChanged at line {line}")
        }
        other => panic!("expected KeyChanged, got {other:?}"),
    }

    // Row: different type, host already has a key -> Ok(false), but known keys are NOT empty.
    // This is the case that must show the "new key type" warning, not a harmless first-connect prompt.
    assert!(matches!(
        check_known_hosts_path("example.test", 22, &ecdsa, &path),
        Ok(false)
    ));
    let known = known_host_keys_path("example.test", 22, &path).unwrap();
    assert_eq!(known.len(), 1);
    println!(
        "different key type:           Ok(false) but host has {} key(s) of another type",
        known.len()
    );

    // Non-default port uses the [host]:port form and is a separate host entry.
    learn_known_hosts_path("example.test", 2222, &ed_b, &path).expect("learn port");
    assert!(matches!(
        check_known_hosts_path("example.test", 2222, &ed_b, &path),
        Ok(true)
    ));
    assert!(matches!(
        check_known_hosts_path("example.test", 22, &ed_a, &path),
        Ok(true)
    ));
    println!("non-default port:             separate entry, both match");

    // Row: unreadable file. DEVIATION: russh swallows any open error and reports "no keys"
    // (Ok(false)), which would show an attacker's key as a first connection. The app must read
    // the file itself first: NotFound means empty, any other error fails the connection.
    // A directory stands in for an unreadable file.
    let result = check_known_hosts_path("example.test", 22, &ed_a, dir.path());
    assert!(
        matches!(result, Ok(false)),
        "russh now errors by itself; simplify the app's pre-check"
    );
    let pre_check = |p: &std::path::Path| match std::fs::read(p) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    };
    assert!(
        pre_check(dir.path()).is_err(),
        "pre-check must reject an unreadable path"
    );
    assert!(
        pre_check(&dir.path().join("missing")).is_ok(),
        "a missing file is just empty"
    );
    println!(
        "unreadable file:              russh says Ok(false) (unsafe); app pre-check rejects it"
    );

    // Corrupt line: record how it is treated.
    let corrupt = dir.path().join("corrupt");
    std::fs::write(&corrupt, "example.test ssh-ed25519 !!!notbase64!!!\n").unwrap();
    println!(
        "corrupt entry:                {:?}",
        check_known_hosts_path("example.test", 22, &ed_a, &corrupt)
    );

    println!("russh host keys OK: every table row behaves as documented");
}
