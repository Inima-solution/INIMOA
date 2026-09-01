//! Database lifecycle: migrate (idempotent) and reset (drop+create+migrate).
//! Reuses the repo's sqlx recipes rather than reimplementing them. (Deterministic
//! data seeding is a local-e2e concern, added back with that flow.)

use std::process::Command;
use std::time::Duration;

use anyhow::{Result, bail};

use super::instance::{Instance, Port};
use super::{stage::Stage, workspace_root};

#[cfg(test)]
mod test;

const READ_WRITE_MAX_ATTEMPTS: usize = 60;
const READ_WRITE_REQUIRED_SUCCESSES: usize = 2;
const READ_WRITE_RETRY_DELAY: Duration = Duration::from_secs(2);
const READ_WRITE_QUERY: &str =
    "SELECT NOT pg_is_in_recovery() AND current_setting('transaction_read_only') = 'off';";

/// Host-side DATABASE_URL for sqlx (binaries run in-container with `postgres`,
/// but host tooling connects via localhost:<mapped-port>).
fn host_database_url(instance: &Instance) -> String {
    format!(
        "postgres://user:password@localhost:{}/macrodb",
        instance.port(Port::Postgres)
    )
}

/// The macro_db_client crate dir (sqlx migrations live under ./migrations).
fn db_client_dir() -> std::path::PathBuf {
    workspace_root().join("crates/macro_db_client")
}

/// Wait until the instance database reports a stable read-write session.
///
/// Compose's `pg_isready` healthcheck proves that Postgres accepts connections,
/// but it also succeeds while the server is in recovery. Local E2E seeding
/// needs the stronger contract before it starts its real database writes.
pub fn wait_read_write(stage: &Stage, instance: &Instance) -> Result<()> {
    stage.run_step("Waiting for writable Postgres", || {
        let ready = wait_until_stable(
            READ_WRITE_MAX_ATTEMPTS,
            READ_WRITE_REQUIRED_SUCCESSES,
            || {
                read_write_probe_command(instance)
                    .output()
                    .is_ok_and(|output| {
                        output.status.success() && output.stdout.trim_ascii() == b"t"
                    })
            },
            || std::thread::sleep(READ_WRITE_RETRY_DELAY),
        );
        if !ready {
            bail!("Postgres did not become ready for local E2E writes in time");
        }
        Ok(())
    })
}

fn read_write_probe_command(instance: &Instance) -> Command {
    let mut command = Command::new("psql");
    command
        .args([
            "--no-psqlrc",
            "--tuples-only",
            "--no-align",
            "--quiet",
            "--set=ON_ERROR_STOP=1",
            "--command",
            READ_WRITE_QUERY,
        ])
        .env("PGHOST", "localhost")
        .env("PGPORT", instance.port(Port::Postgres).to_string())
        .env("PGUSER", "user")
        .env("PGPASSWORD", "password")
        .env("PGDATABASE", "macrodb");
    command
}

fn wait_until_stable(
    max_attempts: usize,
    required_consecutive: usize,
    mut probe: impl FnMut() -> bool,
    mut pause: impl FnMut(),
) -> bool {
    let mut consecutive = 0;
    for attempt in 0..max_attempts {
        if probe() {
            consecutive += 1;
            if consecutive >= required_consecutive {
                return true;
            }
        } else {
            consecutive = 0;
        }
        if attempt + 1 < max_attempts {
            pause();
        }
    }
    false
}

/// Create the database (idempotent) and run migrations.
pub fn migrate(stage: &Stage, instance: &Instance) -> Result<()> {
    let url = host_database_url(instance);

    let mut create = Command::new("sqlx");
    create
        .arg("database")
        .arg("create")
        .env("DATABASE_URL", &url);
    stage.run("Creating database (if needed)", &mut create)?;

    let mut migrate = Command::new("sqlx");
    migrate
        .arg("migrate")
        .arg("run")
        .current_dir(db_client_dir())
        .env("DATABASE_URL", &url);
    stage.run("Running migrations", &mut migrate)
}

/// Drop + recreate + migrate the instance database.
pub fn reset(stage: &Stage, instance: &Instance) -> Result<()> {
    let url = host_database_url(instance);
    let mut drop = Command::new("sqlx");
    drop.arg("database")
        .arg("drop")
        .arg("-y")
        .env("DATABASE_URL", &url);
    let _ = stage.run("Dropping database", &mut drop);
    migrate(stage, instance)
}
