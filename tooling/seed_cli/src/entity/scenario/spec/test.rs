use super::*;

fn example_json() -> String {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("seed/scenarios/team-perms.json");
    std::fs::read_to_string(path).expect("example scenario file exists")
}

fn example() -> ScenarioSpec {
    ScenarioSpec::parse(&example_json()).expect("example scenario is valid")
}

fn inimoa_personas() -> ScenarioSpec {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("seed/scenarios/inimoa-personas.json");
    ScenarioSpec::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn minimal(json: serde_json::Value) -> Result<ScenarioSpec, anyhow::Error> {
    ScenarioSpec::parse(&json.to_string())
}

#[test]
fn derive_id_is_deterministic_and_marked() {
    let a = derive_id("team-perms", "document", "q3-plan");
    let b = derive_id("team-perms", "document", "q3-plan");
    assert_eq!(a, b);

    let other_key = derive_id("team-perms", "document", "handbook");
    let other_kind = derive_id("team-perms", "chat", "q3-plan");
    let other_scenario = derive_id("other", "document", "q3-plan");
    assert_ne!(a, other_key);
    assert_ne!(a, other_kind);
    assert_ne!(a, other_scenario);

    let text = a.to_string();
    assert!(
        text.starts_with(SEED_MARKER),
        "id {text} carries the marker"
    );
    assert!(
        text.starts_with(&scenario_marker("team-perms")),
        "id {text} carries the scenario marker {}",
        scenario_marker("team-perms")
    );
    assert_eq!(a.get_version_num(), 8);
}

#[test]
fn inimoa_personas_are_deterministic_and_keep_member_derived() {
    let spec = inimoa_personas();
    assert_eq!(spec.users.len(), 7);
    assert_eq!(spec.bots.len(), 1);
    assert!(spec.users.contains_key("member"));
    assert!(spec.bots.contains_key("agent"));
    assert_eq!(spec.bot_id("agent"), spec.bot_id("agent"));
    assert!(
        spec.bot_id("agent")
            .to_string()
            .starts_with(&scenario_marker(&spec.scenario))
    );
    assert_eq!(
        spec.bot_principal("agent"),
        format!("bot|{}", spec.bot_id("agent"))
    );
    assert!(
        !spec
            .business_role_assignments
            .iter()
            .any(|row| row.role == models_team::BusinessRole::Member)
    );
    for (key, role) in [
        ("manager", models_team::BusinessRole::Manager),
        ("approver", models_team::BusinessRole::Approver),
        ("hr-admin", models_team::BusinessRole::HrAdmin),
        ("payroll-admin", models_team::BusinessRole::PayrollAdmin),
        ("org-admin", models_team::BusinessRole::OrgAdmin),
        ("auditor", models_team::BusinessRole::Auditor),
    ] {
        assert!(
            spec.business_role_assignments
                .iter()
                .any(|row| row.principal == format!("user:{key}") && row.role == role)
        );
    }
    assert!(spec.business_role_assignments.iter().any(|row| row.principal == "bot:agent" && row.role == models_team::BusinessRole::Agent));
}

#[test]
fn rejects_invalid_business_role_personas() {
    let fixture = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("seed/scenarios/inimoa-personas.json"),
    )
    .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&fixture).unwrap();
    let rows = value["business_role_assignments"].as_array_mut().unwrap();
    rows.push(serde_json::json!({"team":"missing", "principal":"user:ghost", "role":"member"}));
    rows.push(serde_json::json!({"team":"inimoa", "principal":"user:manager", "role":"agent"}));
    rows.push(serde_json::json!({"team":"inimoa", "principal":"bot:agent", "role":"manager"}));
    rows.push(serde_json::json!({"team":"inimoa", "principal":"nope", "role":"manager"}));
    rows.push(serde_json::json!({"team":"inimoa", "principal":"user:manager", "role":"manager"}));
    let error = ScenarioSpec::parse(&value.to_string())
        .unwrap_err()
        .to_string();
    for expected in [
        "unknown team",
        "unknown user",
        "member` is membership-derived",
        "cannot receive business role `agent`",
        "may only receive business role `agent`",
        "must be user:<key> or bot:<key>",
        "duplicate business role assignment",
    ] {
        assert!(error.contains(expected), "missing {expected}: {error}");
    }
}

#[test]
fn rejects_bot_without_its_required_agent_assignment() {
    let fixture = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("seed/scenarios/inimoa-personas.json"),
    )
    .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&fixture).unwrap();
    value["business_role_assignments"]
        .as_array_mut()
        .unwrap()
        .retain(|row| row["principal"] != "bot:agent");

    let error = ScenarioSpec::parse(&value.to_string())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("bot `agent` must receive exactly one `agent` business role"),
        "{error}"
    );
}

#[test]
fn rejects_bot_business_role_for_a_non_owning_team() {
    let fixture = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("seed/scenarios/inimoa-personas.json"),
    )
    .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&fixture).unwrap();
    value["teams"]["other"] = serde_json::json!({"owner": "org-admin"});
    let assignment = value["business_role_assignments"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["principal"] == "bot:agent")
        .unwrap();
    assignment["team"] = serde_json::json!("other");

    let error = ScenarioSpec::parse(&value.to_string())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("bot `agent` is not owned by team `other`"),
        "{error}"
    );
}

#[test]
fn scenario_marker_shape() {
    let marker = scenario_marker("team-perms");
    assert_eq!(marker.len(), 8);
    assert!(marker.starts_with(SEED_MARKER));
    assert_ne!(marker, scenario_marker("other-scenario"));
}

#[test]
fn example_scenario_parses() {
    let spec = example();
    assert_eq!(spec.scenario, "team-perms");
    assert_eq!(spec.users.len(), 6);
    assert_eq!(spec.user_id("alice"), "macro|alice@seed.macro.local");

    let handbook = &spec.documents["handbook"];
    assert_eq!(handbook.link_share, Some(LinkShare::Public));
    assert_eq!(handbook.link_share_access_level, Some(ShareLevel::View));

    let bob_notes = &spec.documents["bob-notes"];
    assert_eq!(bob_notes.link_share, Some(LinkShare::Team));
    assert_eq!(bob_notes.link_share_access_level, Some(ShareLevel::View));
}

#[test]
fn team_and_channel_derivations() {
    let spec = example();

    assert_eq!(spec.team_of("alice"), Some("acme"));
    assert_eq!(spec.team_of("bob"), Some("acme"));
    assert_eq!(spec.team_of("dave"), None);

    let mut hq_members = spec.channel_members("acme-hq");
    hq_members.sort();
    assert_eq!(hq_members, vec!["alice", "bob", "carol", "erin"]);
    assert_eq!(spec.channel_owner("acme-hq"), "alice");

    assert_eq!(spec.channel_owner("dm-alice-dave"), "alice");
    let mut eng_members = spec.channel_members("eng");
    eng_members.sort();
    assert_eq!(eng_members, vec!["alice", "bob", "dave"]);
}

#[test]
fn call_participants_include_creator_once() {
    let spec = example();
    assert_eq!(spec.call_participants("eng-standup"), vec!["alice", "bob"]);
    assert_eq!(spec.call_participants("dm-huddle"), vec!["dave", "alice"]);
}

#[test]
fn rejects_unknown_references() {
    let result = minimal(serde_json::json!({
        "scenario": "bad",
        "users": { "alice": { "email": "alice@x.local" } },
        "documents": {
            "doc": {
                "owner": "ghost",
                "share": [ { "with": "team:none", "level": "view" } ]
            }
        }
    }));
    let error = result.unwrap_err().to_string();
    assert!(error.contains("unknown user `ghost`"), "{error}");
    assert!(error.contains("unknown team `none`"), "{error}");
}

#[test]
fn rejects_second_team_membership() {
    let result = minimal(serde_json::json!({
        "scenario": "bad",
        "users": {
            "alice": { "email": "alice@x.local" },
            "bob": { "email": "bob@x.local" },
            "carol": { "email": "carol@x.local" }
        },
        "teams": {
            "one": { "owner": "alice", "members": { "bob": "member" } },
            "two": { "owner": "carol", "members": { "bob": "admin" } }
        }
    }));
    let error = result.unwrap_err().to_string();
    assert!(error.contains("one team"), "{error}");
}

#[test]
fn rejects_bad_channels() {
    let result = minimal(serde_json::json!({
        "scenario": "bad",
        "users": {
            "alice": { "email": "alice@x.local" },
            "bob": { "email": "bob@x.local" }
        },
        "teams": { "acme": { "owner": "alice" } },
        "channels": {
            "dm": { "type": "direct_message", "members": ["alice"] },
            "hq": { "type": "team", "team": "acme", "members": ["bob"] },
            "orphan": { "type": "private" }
        }
    }));
    let error = result.unwrap_err().to_string();
    assert!(error.contains("exactly two distinct members"), "{error}");
    assert!(
        error.contains("derives owner/members from the team"),
        "{error}"
    );
    assert!(error.contains("must set `owner`"), "{error}");
}

#[test]
fn rejects_call_and_message_membership_violations() {
    let result = minimal(serde_json::json!({
        "scenario": "bad",
        "users": {
            "alice": { "email": "alice@x.local" },
            "eve": { "email": "eve@x.local" }
        },
        "channels": {
            "eng": { "type": "private", "owner": "alice" }
        },
        "calls": {
            "call": { "channel": "eng", "created_by": "eve" }
        },
        "messages": [
            { "channel": "eng", "from": "eve", "text": "hi" }
        ]
    }));
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("created_by `eve` is not a member"),
        "{error}"
    );
    assert!(error.contains("sender `eve` is not a member"), "{error}");
}

#[test]
fn rejects_project_cycles() {
    let result = minimal(serde_json::json!({
        "scenario": "bad",
        "users": { "alice": { "email": "alice@x.local" } },
        "projects": {
            "a": { "owner": "alice", "parent": "b" },
            "b": { "owner": "alice", "parent": "a" }
        }
    }));
    let error = result.unwrap_err().to_string();
    assert!(error.contains("parent cycle"), "{error}");
}

#[test]
fn rejects_unknown_roles() {
    let result = minimal(serde_json::json!({
        "scenario": "bad",
        "users": {
            "alice": { "email": "alice@x.local", "roles": ["professional_subscriber", "royalty"] }
        }
    }));
    let error = result.unwrap_err().to_string();
    assert!(error.contains("unknown role `royalty`"), "{error}");
    assert!(!error.contains("professional_subscriber"), "{error}");
}

#[test]
fn rejects_unknown_fields() {
    let result = minimal(serde_json::json!({
        "scenario": "bad",
        "users": { "alice": { "email": "alice@x.local", "surprise": true } }
    }));
    assert!(result.unwrap_err().to_string().contains("unknown field"));
}

#[test]
fn rejects_incomplete_link_share_policies() {
    let missing_access_level = minimal(serde_json::json!({
        "scenario": "bad",
        "users": { "alice": { "email": "alice@x.local" } },
        "documents": {
            "doc": { "owner": "alice", "link_share": "TEAM" }
        }
    }));
    let error = missing_access_level.unwrap_err().to_string();
    assert!(
        error.contains("sets link_share but not link_share_access_level"),
        "{error}"
    );

    let missing_scope = minimal(serde_json::json!({
        "scenario": "bad",
        "users": { "alice": { "email": "alice@x.local" } },
        "chats": {
            "chat": { "owner": "alice", "link_share_access_level": "view" }
        }
    }));
    let error = missing_scope.unwrap_err().to_string();
    assert!(
        error.contains("sets link_share_access_level but not link_share"),
        "{error}"
    );
}

#[test]
fn rejects_legacy_public_link_field() {
    let result = minimal(serde_json::json!({
        "scenario": "bad",
        "users": { "alice": { "email": "alice@x.local" } },
        "projects": {
            "project": { "owner": "alice", "public": "view" }
        }
    }));
    assert!(result.unwrap_err().to_string().contains("unknown field"));
}

#[test]
fn entity_ref_parsing() {
    assert_eq!(
        EntityRef::parse("user:alice").unwrap(),
        EntityRef::User("alice".to_string())
    );
    assert_eq!(
        EntityRef::parse("document:q3-plan").unwrap(),
        EntityRef::Document("q3-plan".to_string())
    );
    assert!(EntityRef::parse("nope").is_err());
    assert!(EntityRef::parse("user:").is_err());
    assert!(EntityRef::parse("widget:x").is_err());
}

#[test]
fn project_chain_walks_parents() {
    let spec = minimal(serde_json::json!({
        "scenario": "chain",
        "users": { "alice": { "email": "alice@x.local" } },
        "projects": {
            "root": { "owner": "alice" },
            "mid": { "owner": "alice", "parent": "root" },
            "leaf": { "owner": "alice", "parent": "mid" }
        }
    }))
    .unwrap();
    assert_eq!(spec.project_chain("leaf"), vec!["leaf", "mid", "root"]);
    assert_eq!(spec.project_chain("root"), vec!["root"]);
}
