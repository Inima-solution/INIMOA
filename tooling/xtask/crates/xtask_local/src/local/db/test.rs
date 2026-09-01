use std::ffi::OsStr;

use super::*;

#[test]
fn read_write_probe_targets_the_instance_without_credentials_in_arguments() {
    let instance = Instance::derive(Some("db-ready-test"), Some(31_000)).unwrap();
    let command = read_write_probe_command(&instance);
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>();

    assert_eq!(command.get_program(), OsStr::new("psql"));
    assert!(
        arguments
            .iter()
            .any(|argument| argument.contains("pg_is_in_recovery"))
    );
    assert!(
        !arguments
            .iter()
            .any(|argument| argument.contains("password"))
    );
    assert!(
        !arguments
            .iter()
            .any(|argument| argument.contains("postgres://"))
    );

    let env = command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(env["PGHOST"].as_deref(), Some("localhost"));
    assert_eq!(env["PGPORT"].as_deref(), Some("31000"));
    assert_eq!(env["PGUSER"].as_deref(), Some("user"));
    assert_eq!(env["PGDATABASE"].as_deref(), Some("macrodb"));
}

#[test]
fn readiness_requires_consecutive_success_and_stays_bounded() {
    let mut results = [true, false, true, true].into_iter();
    let mut probes = 0;
    let mut pauses = 0;

    let ready = wait_until_stable(
        6,
        2,
        || {
            probes += 1;
            results.next().unwrap_or(false)
        },
        || pauses += 1,
    );

    assert!(ready);
    assert_eq!(probes, 4);
    assert_eq!(pauses, 3);

    let mut probes = 0;
    let mut pauses = 0;
    let ready = wait_until_stable(
        3,
        2,
        || {
            probes += 1;
            false
        },
        || pauses += 1,
    );

    assert!(!ready);
    assert_eq!(probes, 3);
    assert_eq!(pauses, 2);
}
