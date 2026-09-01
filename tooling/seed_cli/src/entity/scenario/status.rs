//! Status: report which of a scenario's rows are actually present — per-kind
//! counts, FusionAuth accounts, sync-service content — and re-print the
//! persona login links. Read-only.
//!
//! Without `--file` it discovers applied scenarios instead: seeded ids carry
//! `5eed` + a 4-hex-char hash of the scenario name, so grouping marked rows
//! by their first 8 uuid chars yields one bucket per applied scenario, which
//! is then matched against the names declared in `seed/scenarios/*.json`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use anyhow::Context;
use models_team::BusinessRole;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::SeedCliContext;

use super::spec::{SEED_MARKER, ScenarioSpec, scenario_marker};

/// The tables holding marker-prefixed container rows, as (kind, table, id
/// column). Documents and tasks share the `"Document"` table.
const MARKED_TABLES: &[(&str, &str, &str)] = &[
    ("teams", "team", "id"),
    ("bots", "bots", "id"),
    ("channels", "comms_channels", "id"),
    ("projects", "\"Project\"", "id"),
    ("documents", "\"Document\"", "id"),
    ("chats", "\"Chat\"", "id"),
    ("calls", "call_records", "id"),
    ("inboxes", "email_links", "id"),
    ("messages", "comms_messages", "id"),
];

/// One kind's presence check: the expected `(spec key, id)` pairs and where
/// their rows live.
struct KindCheck {
    label: &'static str,
    table: &'static str,
    id_column: &'static str,
    expected: Vec<(String, String)>,
}

/// One exact stored role expected after applying a validated scenario.
/// Membership's `member` role is deliberately absent because it is derived.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BusinessRoleCheck {
    team_id: Uuid,
    /// Scenario-local `user:<key>` or `bot:<key>` label, safe to display.
    scenario_principal: String,
    /// Canonical storage principal used only as a parameterized query key.
    principal: String,
    role: BusinessRole,
}

/// Scenario-owned principals whose stored roles must be exact for one team.
#[derive(Debug)]
struct BusinessRoleScope {
    team_id: Uuid,
    /// Canonical storage principal -> safe scenario-local display label.
    principals: BTreeMap<String, String>,
}

fn business_role_checks(spec: &ScenarioSpec) -> Vec<BusinessRoleCheck> {
    spec.business_role_assignments
        .iter()
        .map(|assignment| {
            let principal = if let Some(user) = assignment.principal.strip_prefix("user:") {
                spec.user_id(user)
            } else if let Some(bot) = assignment.principal.strip_prefix("bot:") {
                spec.bot_principal(bot)
            } else {
                unreachable!("scenario validation rejects unknown principal kinds")
            };
            BusinessRoleCheck {
                team_id: spec.team_id(&assignment.team),
                scenario_principal: assignment.principal.clone(),
                principal,
                role: assignment.role,
            }
        })
        .collect()
}

fn business_role_scopes(spec: &ScenarioSpec) -> Vec<BusinessRoleScope> {
    spec.teams
        .iter()
        .map(|(team_key, team)| {
            let mut principals = BTreeMap::new();
            for user in std::iter::once(&team.owner).chain(team.members.keys()) {
                principals.insert(spec.user_id(user), format!("user:{user}"));
            }
            for bot_key in spec
                .bots
                .iter()
                .filter(|(_, bot)| bot.team == *team_key)
                .map(|(key, _)| key)
            {
                principals.insert(spec.bot_principal(bot_key), format!("bot:{bot_key}"));
            }
            BusinessRoleScope {
                team_id: spec.team_id(team_key),
                principals,
            }
        })
        .collect()
}

fn business_role_key(check: &BusinessRoleCheck) -> (String, String, String) {
    (
        check.team_id.to_string(),
        check.principal.clone(),
        check.role.to_string(),
    )
}

fn business_role_label(check: &BusinessRoleCheck) -> String {
    format!("{}:{}", check.scenario_principal, check.role)
}

fn missing_business_roles(
    expected: &[BusinessRoleCheck],
    found: &BTreeSet<(String, String, String)>,
) -> Vec<String> {
    expected
        .iter()
        .filter(|check| !found.contains(&business_role_key(check)))
        .map(business_role_label)
        .collect()
}

fn unexpected_business_roles(
    expected: &[BusinessRoleCheck],
    found: &BTreeSet<(String, String, String)>,
    scopes: &[BusinessRoleScope],
) -> Vec<String> {
    found
        .iter()
        .filter(|found| {
            !expected
                .iter()
                .any(|check| business_role_key(check) == **found)
        })
        .filter_map(|(team_id, principal, role)| {
            scopes
                .iter()
                .find(|scope| scope.team_id.to_string() == *team_id)
                .and_then(|scope| principals_label(&scope.principals, principal))
                .map(|label| format!("{label}:{role}"))
        })
        .collect()
}

fn principals_label<'a>(
    principals: &'a BTreeMap<String, String>,
    principal: &str,
) -> Option<&'a str> {
    principals.get(principal).map(String::as_str)
}

fn kind_checks(spec: &ScenarioSpec) -> Vec<KindCheck> {
    let pairs = |keys: Vec<(&String, String)>| {
        keys.into_iter()
            .map(|(key, id)| (key.clone(), id))
            .collect::<Vec<_>>()
    };
    vec![
        KindCheck {
            label: "bots",
            table: "bots",
            id_column: "id",
            expected: pairs(
                spec.bots
                    .keys()
                    .map(|k| (k, spec.bot_id(k).to_string()))
                    .collect(),
            ),
        },
        KindCheck {
            label: "teams",
            table: "team",
            id_column: "id",
            expected: pairs(
                spec.teams
                    .keys()
                    .map(|k| (k, spec.team_id(k).to_string()))
                    .collect(),
            ),
        },
        KindCheck {
            label: "channels",
            table: "comms_channels",
            id_column: "id",
            expected: pairs(
                spec.channels
                    .keys()
                    .map(|k| (k, spec.channel_id(k).to_string()))
                    .collect(),
            ),
        },
        KindCheck {
            label: "projects",
            table: "\"Project\"",
            id_column: "id",
            expected: pairs(
                spec.projects
                    .keys()
                    .map(|k| (k, spec.project_id(k)))
                    .collect(),
            ),
        },
        KindCheck {
            label: "documents",
            table: "\"Document\"",
            id_column: "id",
            expected: pairs(
                spec.documents
                    .keys()
                    .map(|k| (k, spec.document_id(k)))
                    .collect(),
            ),
        },
        KindCheck {
            label: "tasks",
            table: "\"Document\"",
            id_column: "id",
            expected: pairs(spec.tasks.keys().map(|k| (k, spec.task_id(k))).collect()),
        },
        KindCheck {
            label: "chats",
            table: "\"Chat\"",
            id_column: "id",
            expected: pairs(spec.chats.keys().map(|k| (k, spec.chat_id(k))).collect()),
        },
        KindCheck {
            label: "calls",
            table: "call_records",
            id_column: "id",
            expected: pairs(
                spec.calls
                    .keys()
                    .map(|k| (k, spec.call_id(k).to_string()))
                    .collect(),
            ),
        },
        KindCheck {
            label: "inboxes",
            table: "email_links",
            id_column: "id",
            expected: pairs(
                spec.emails
                    .keys()
                    .map(|k| (k, spec.email_link_id(k).to_string()))
                    .collect(),
            ),
        },
        KindCheck {
            label: "messages",
            table: "comms_messages",
            id_column: "id",
            expected: spec
                .messages
                .iter()
                .enumerate()
                .map(|(index, message)| {
                    (
                        format!("#{index} in `{}`", message.channel),
                        spec.message_id(index).to_string(),
                    )
                })
                .collect(),
        },
    ]
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn inimoa_persona_status_includes_the_agent_bot() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("seed/scenarios/inimoa-personas.json");
        let spec = ScenarioSpec::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        let bots = kind_checks(&spec)
            .into_iter()
            .find(|check| check.label == "bots")
            .unwrap();
        assert_eq!(
            bots.expected,
            vec![("agent".to_string(), spec.bot_id("agent").to_string())]
        );
    }

    #[test]
    fn business_role_status_requires_the_exact_canonical_principal_and_role() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("seed/scenarios/inimoa-personas.json");
        let spec = ScenarioSpec::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        let expected = business_role_checks(&spec);
        let scopes = business_role_scopes(&spec);
        let agent = expected
            .iter()
            .find(|check| check.principal == spec.bot_principal("agent"))
            .unwrap();

        assert_eq!(agent.role, BusinessRole::Agent);
        assert_eq!(business_role_label(agent), "bot:agent:agent");
        assert!(!business_role_label(agent).contains("bot|"));
        assert_eq!(expected.len(), 7);

        let found = expected.iter().map(business_role_key).collect();
        assert!(missing_business_roles(&expected, &found).is_empty());

        let mut missing = found.clone();
        missing.remove(&business_role_key(agent));
        assert_eq!(
            missing_business_roles(&expected, &missing),
            vec![business_role_label(agent)]
        );

        let mut wrong_role = missing;
        wrong_role.insert((
            agent.team_id.to_string(),
            agent.principal.clone(),
            BusinessRole::Manager.to_string(),
        ));
        assert_eq!(
            missing_business_roles(&expected, &wrong_role),
            vec![business_role_label(agent)]
        );
        assert_eq!(
            unexpected_business_roles(&expected, &wrong_role, &scopes),
            vec!["bot:agent:manager"]
        );

        let member_principal = spec.user_id("member");
        let mut extra_role = found;
        extra_role.insert((
            spec.team_id("inimoa").to_string(),
            member_principal,
            BusinessRole::Manager.to_string(),
        ));
        assert!(missing_business_roles(&expected, &extra_role).is_empty());
        assert_eq!(
            unexpected_business_roles(&expected, &extra_role, &scopes),
            vec!["user:member:manager"]
        );
    }

    #[test]
    fn zero_expected_business_roles_still_reject_a_stale_role() {
        let spec = ScenarioSpec::parse(
            r#"{
                "scenario":"human-only",
                "users":{"member":{"email":"member@human-only.local"}},
                "teams":{"team":{"owner":"member"}}
            }"#,
        )
        .unwrap();
        let expected = business_role_checks(&spec);
        let scopes = business_role_scopes(&spec);
        assert!(expected.is_empty());

        let found = BTreeSet::from([(
            spec.team_id("team").to_string(),
            spec.user_id("member"),
            BusinessRole::Manager.to_string(),
        )]);
        assert_eq!(
            unexpected_business_roles(&expected, &found, &scopes),
            vec!["user:member:manager"]
        );
    }
}

/// The ids of every expected `SELECT` hit in `table.column`, as text.
#[allow(clippy::disallowed_methods, reason = "seed-only dynamic SQL")]
async fn existing_ids(
    pool: &PgPool,
    table: &str,
    column: &str,
    ids: &[String],
) -> anyhow::Result<HashSet<String>> {
    let sql = format!("SELECT {column}::text FROM {table} WHERE {column}::text = ANY($1)");
    let found: Vec<String> = sqlx::query_scalar(&sql)
        .bind(ids)
        .fetch_all(pool)
        .await
        .with_context(|| format!("querying {table}"))?;
    Ok(found.into_iter().collect())
}

/// Look up exact, scenario-derived stored role rows. This is read-only and
/// parameterized because role status must not infer success from the bot row.
#[allow(
    clippy::disallowed_methods,
    reason = "seed-only parameterized status query; SQLx macros require unavailable offline metadata"
)]
async fn existing_business_roles(
    pool: &PgPool,
    scopes: &[BusinessRoleScope],
) -> anyhow::Result<BTreeSet<(String, String, String)>> {
    let mut found = BTreeSet::new();
    for scope in scopes {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT principal, business_role::text FROM team_business_role \
             WHERE team_id = $1 AND principal = ANY($2)",
        )
        .bind(scope.team_id)
        .bind(scope.principals.keys().cloned().collect::<Vec<_>>())
        .fetch_all(pool)
        .await
        .context("querying team_business_role")?;
        for (principal, role) in rows {
            found.insert((scope.team_id.to_string(), principal, role));
        }
    }
    Ok(found)
}

fn print_check_line(label: &str, found: usize, expected: usize, missing: &[String], extra: &str) {
    let mut line = format!("  {label:<13} {found}/{expected}{extra}");
    if !missing.is_empty() {
        line.push_str(&format!(" (missing: {})", missing.join(", ")));
    }
    println!("{line}");
}

/// Report presence for one scenario file's spec.
pub async fn report(ctx: &SeedCliContext, spec: &ScenarioSpec) -> anyhow::Result<()> {
    let pool = ctx.db.pool();
    println!(
        "Scenario `{}` (marker {})",
        spec.scenario,
        scenario_marker(&spec.scenario)
    );

    let mut expected_total = 0usize;
    let mut found_total = 0usize;

    // Users carry no marker (their rows come from the signup webhook), so
    // presence is keyed by email; FusionAuth is checked alongside.
    if !spec.users.is_empty() {
        let emails: Vec<String> = spec.users.values().map(|u| u.email.clone()).collect();
        let found = existing_ids(&pool, "\"User\"", "email", &emails).await?;
        let missing: Vec<String> = spec
            .users
            .iter()
            .filter(|(_, user)| !found.contains(&user.email))
            .map(|(key, _)| key.clone())
            .collect();

        let mut fa_found = 0usize;
        for email in &emails {
            if ctx
                .fusionauth_client
                .user_exists(email)
                .await
                .unwrap_or(false)
            {
                fa_found += 1;
            }
        }
        let fa_hint = if fa_found == 0 {
            " (accounts missing, or FusionAuth is unreachable)"
        } else {
            ""
        };
        print_check_line(
            "users",
            found.len(),
            emails.len(),
            &missing,
            &format!("   fusionauth {fa_found}/{}{fa_hint}", emails.len()),
        );
        expected_total += emails.len();
        found_total += found.len();
    }

    for check in kind_checks(spec) {
        if check.expected.is_empty() {
            continue;
        }
        let ids: Vec<String> = check.expected.iter().map(|(_, id)| id.clone()).collect();
        let found = existing_ids(&pool, check.table, check.id_column, &ids).await?;
        let missing: Vec<String> = check
            .expected
            .iter()
            .filter(|(_, id)| !found.contains(id))
            .map(|(key, _)| key.clone())
            .collect();
        print_check_line(check.label, found.len(), ids.len(), &missing, "");
        expected_total += ids.len();
        found_total += found.len();
    }

    let role_checks = business_role_checks(spec);
    let role_scopes = business_role_scopes(spec);
    let mut has_unexpected_roles = false;
    if !role_scopes.is_empty() {
        let found = existing_business_roles(&pool, &role_scopes).await?;
        let missing = missing_business_roles(&role_checks, &found);
        let unexpected = unexpected_business_roles(&role_checks, &found, &role_scopes);
        has_unexpected_roles = !unexpected.is_empty();
        let expected_found = role_checks.len() - missing.len();
        let unexpected_hint = if unexpected.is_empty() {
            String::new()
        } else {
            format!(" (unexpected: {})", unexpected.join(", "))
        };
        print_check_line(
            "business roles",
            expected_found,
            role_checks.len(),
            &missing,
            &unexpected_hint,
        );
        expected_total += role_checks.len();
        found_total += expected_found;
    }

    report_sync_content(ctx, spec).await;

    if found_total == 0 {
        println!("\nNot applied — run `just seed-scenario apply --file <scenario.json>`.");
    } else if found_total < expected_total || has_unexpected_roles {
        println!("\nPartially applied — re-run apply to converge. Log in as:");
        super::apply::print_login_links(spec);
    } else {
        println!("\nFully applied. Log in as:");
        super::apply::print_login_links(spec);
    }
    Ok(())
}

/// Probe sync-service for the entities whose content lives there: every
/// task, plus documents with inline content or a `.md` file.
async fn report_sync_content(ctx: &SeedCliContext, spec: &ScenarioSpec) {
    let mut targets: Vec<(String, String)> = spec
        .documents
        .iter()
        .filter(|(_, doc)| {
            doc.content.is_some() || doc.file.as_deref().is_some_and(|f| f.ends_with(".md"))
        })
        .map(|(key, _)| (key.clone(), spec.document_id(key)))
        .collect();
    targets.extend(
        spec.tasks
            .keys()
            .map(|key| (key.clone(), spec.task_id(key))),
    );
    if targets.is_empty() {
        return;
    }

    let Some(clients) = ctx.doc_content.as_ref() else {
        println!("  sync content  skipped (SYNC_SERVICE_URL/LEXICAL_SERVICE_URL unset)");
        return;
    };
    let mut missing = Vec::new();
    for (key, id) in &targets {
        if clients.sync.get_raw(id).await.is_err() {
            missing.push(key.clone());
        }
    }
    print_check_line(
        "sync content",
        targets.len() - missing.len(),
        targets.len(),
        &missing,
        "",
    );
}

/// No `--file`: find every applied scenario by scanning for marked ids, then
/// report fully on the ones whose name matches a file in `scenarios_dir`.
#[allow(clippy::disallowed_methods, reason = "seed-only dynamic SQL")]
pub async fn discover(ctx: &SeedCliContext, scenarios_dir: &Path) -> anyhow::Result<()> {
    let pool = ctx.db.pool();

    let mut counts: BTreeMap<String, Vec<(&'static str, i64)>> = BTreeMap::new();
    for (label, table, column) in MARKED_TABLES {
        let sql = format!(
            "SELECT substring({column}::text, 1, 8), count(*) FROM {table} \
             WHERE {column}::text LIKE '{SEED_MARKER}%' GROUP BY 1"
        );
        let rows: Vec<(String, i64)> = sqlx::query_as(&sql)
            .fetch_all(&pool)
            .await
            .with_context(|| format!("scanning {table}"))?;
        for (marker, count) in rows {
            counts.entry(marker).or_default().push((label, count));
        }
    }

    if counts.is_empty() {
        println!("No seeded rows found (no ids starting `{SEED_MARKER}`).");
        return Ok(());
    }

    let known = known_scenarios(scenarios_dir);
    let mut first = true;
    for (marker, tables) in &counts {
        if !first {
            println!();
        }
        first = false;
        match known.get(marker) {
            Some((file, spec)) => {
                println!("{}:", file.display());
                report(ctx, spec).await?;
            }
            None => {
                let breakdown = tables
                    .iter()
                    .map(|(label, count)| format!("{count} {label}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "Unknown scenario (marker {marker}): {breakdown} — no file in {} declares a matching name; `reset --all` removes it",
                    scenarios_dir.display()
                );
            }
        }
    }
    Ok(())
}

/// Parse every readable scenario file in `dir` into a marker -> (path, spec)
/// map. Unreadable or invalid files are skipped: discovery should still
/// report the database's state.
fn known_scenarios(dir: &Path) -> BTreeMap<String, (std::path::PathBuf, ScenarioSpec)> {
    let mut known = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return known;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(spec) = ScenarioSpec::parse(&content) else {
            continue;
        };
        known.insert(scenario_marker(&spec.scenario), (path, spec));
    }
    known
}
