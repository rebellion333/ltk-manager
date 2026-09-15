use super::*;

#[test]
fn parses_the_five_part_form_every_current_client_writes() {
    let lf = LeagueLockfile::parse("LeagueClient:12345:54321:sEcReT:https\n").unwrap();
    assert_eq!(lf.name, "LeagueClient");
    assert_eq!(lf.pid, 12345);
    assert_eq!(lf.port, 54321);
    assert_eq!(lf.password, "sEcReT");
    assert_eq!(lf.protocol, "https");
    assert_eq!(lf.base_url(), "https://127.0.0.1:54321");
}

#[test]
fn parses_the_four_part_form_without_a_pid() {
    let lf = LeagueLockfile::parse("LeagueClient:54321:pw:https").unwrap();
    assert_eq!(lf.pid, 0);
    assert_eq!(lf.port, 54321);
    assert!(
        lf.is_live(),
        "a lockfile naming no pid is taken at its word"
    );
}

#[test]
fn refuses_anything_else() {
    assert!(LeagueLockfile::parse("").is_none());
    assert!(LeagueLockfile::parse("LeagueClient:notaport:pw:https").is_none());
    assert!(LeagueLockfile::parse("a:b:c:d:e:f").is_none());
}

#[test]
fn debug_never_prints_the_password() {
    let lf = LeagueLockfile::parse("LeagueClient:1:2:hunter2:https").unwrap();
    let printed = format!("{lf:?}");
    assert!(!printed.contains("hunter2"));
    assert!(printed.contains("redacted"));
}

#[test]
fn the_current_process_is_live_and_a_dead_pid_is_not() {
    let me = LeagueLockfile {
        name: "x".into(),
        pid: std::process::id(),
        port: 1,
        password: String::new(),
        protocol: "https".into(),
    };
    assert!(me.is_live());

    // The largest pid Windows hands out is far below this, and Linux caps at
    // 2^22 by default.
    let gone = LeagueLockfile {
        pid: u32::MAX - 7,
        ..me
    };
    assert!(!gone.is_live());
}

#[test]
fn a_missing_file_reads_as_none() {
    let dir = tempfile::tempdir().unwrap();
    assert!(LeagueLockfile::read(dir.path()).is_none());
    fs_err::write(dir.path().join("lockfile"), "LeagueClient:1:2:pw:https").unwrap();
    assert_eq!(LeagueLockfile::read(dir.path()).unwrap().port, 2);
}
