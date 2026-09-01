use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tempfile::NamedTempFile;

use crate::config::SeedCliContext;
use crate::service::{auth::Auth, db::Db, s3::S3};

use super::*;

fn scenario_file() -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"{{"scenario":"reset-test","users":{{"member":{{"email":"member@reset-test.local"}}}}}}"#
    )
    .unwrap();
    file
}

fn context(auth: Auth, db: Db) -> SeedCliContext {
    SeedCliContext {
        db,
        fusionauth_client: auth,
        s3: S3::default(),
        doc_content: None,
    }
}

#[tokio::test]
async fn file_reset_deletes_fusionauth_before_database_cleanup() {
    let deleted = Arc::new(AtomicUsize::new(0));
    let mut auth = Auth::default();
    let deleted_by_auth = Arc::clone(&deleted);
    auth.expect_delete_user_by_email()
        .times(1)
        .returning(move |_| {
            deleted_by_auth.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
    let mut db = Db::default();
    let deleted_before_db = Arc::clone(&deleted);
    db.expect_execute_sql_if_table_exists()
        .times(1)
        .returning(move |_, _| {
            assert_eq!(deleted_before_db.load(Ordering::SeqCst), 1);
            Ok(())
        });
    db.expect_execute_statements()
        .times(1)
        .returning(|_| Ok(()));
    let file = scenario_file();

    reset_scenario(
        &context(auth, db),
        &ResetScenarioArgs {
            file: Some(file.path().display().to_string()),
            all: false,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn file_reset_fails_before_database_cleanup_when_fusionauth_delete_fails() {
    let mut auth = Auth::default();
    auth.expect_delete_user_by_email()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("auth unavailable")));
    let file = scenario_file();

    assert!(
        reset_scenario(
            &context(auth, Db::default()),
            &ResetScenarioArgs {
                file: Some(file.path().display().to_string()),
                all: false
            },
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn all_reset_remains_database_only() {
    let mut db = Db::default();
    db.expect_execute_sql_if_table_exists()
        .times(1)
        .returning(|_, _| Ok(()));
    db.expect_execute_statements()
        .times(1)
        .returning(|_| Ok(()));

    reset_scenario(
        &context(Auth::default(), db),
        &ResetScenarioArgs {
            file: None,
            all: true,
        },
    )
    .await
    .unwrap();
}
