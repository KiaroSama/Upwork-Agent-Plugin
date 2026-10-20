use serde_json::Value;
use std::{fs, path::Path};

#[test]
fn shared_skill_and_client_manifests_are_complete_without_direct_mcp() {
    let portable: Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    assert_eq!(portable["name"], "upwork-agent-connector");
    assert_eq!(portable["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(portable["license"], "GPL-3.0-only");
    assert_eq!(
        portable["$schema"],
        "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json"
    );
    assert!(portable.get("skills").is_none());
    let marketplace: Value =
        serde_json::from_str(include_str!("../.claude-plugin/marketplace.json")).unwrap();
    assert_eq!(marketplace["name"], "upwork-agent-plugin-marketplace");
    assert_eq!(marketplace["plugins"][0]["source"], "./");
    assert_eq!(marketplace["plugins"][0]["name"], portable["name"]);
    assert_eq!(marketplace["plugins"][0]["version"], portable["version"]);
    let codex: Value = serde_json::from_str(include_str!("../.codex-plugin/plugin.json")).unwrap();
    let interface = &portable["extensions"]["com.openai"]["interface"];
    assert_eq!(interface, &codex["interface"]);
    assert_eq!(interface["capabilities"], serde_json::json!(["Read"]));
    for key in [
        "displayName",
        "shortDescription",
        "developerName",
        "category",
        "websiteURL",
    ] {
        assert!(!interface[key].as_str().unwrap().is_empty());
    }
    assert!(!interface["defaultPrompt"].as_array().unwrap().is_empty());
    for text in [
        include_str!("../.claude-plugin/plugin.json"),
        include_str!("../.codex-plugin/plugin.json"),
        include_str!("../.cursor-plugin/plugin.json"),
    ] {
        let client: Value = serde_json::from_str(text).unwrap();
        for key in ["name", "version", "license", "description", "repository"] {
            assert_eq!(client[key], portable[key]);
        }
        assert_eq!(client["skills"], "./skills/");
        assert!(client.get("mcpServers").is_none());
        assert!(client.get("hooks").is_none());
    }
    assert!(!Path::new("mcp.json").exists());
    let skill = include_str!("../skills/upwork-project-selection/SKILL.md").replace("\r\n", "\n");
    assert!(skill.starts_with("---\nname: upwork-project-selection\ndescription: "));
    assert!(skill.contains("license: GPL-3.0-only\n---"));
    assert!(skill.lines().count() < 500);
    let readme = include_str!("../docs/skills.md");
    assert!(readme.contains("../skills/upwork-project-selection/SKILL.md"));
    for name in ["Claude", "Codex", "Cursor", "Antigravity"] {
        assert!(readme.contains(name));
    }
    let folders = fs::read_dir("skills")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").exists())
        .count();
    assert_eq!(
        folders, 2,
        "New skills need a catalog entry and evaluation coverage"
    );
}

#[test]
fn reusable_evaluations_have_unique_ids_and_no_private_inputs() {
    let evals: Value = serde_json::from_str(include_str!(
        "../skills/upwork-project-selection/evals/evals.json"
    ))
    .unwrap();
    assert_eq!(evals["skill_name"], "upwork-project-selection");
    let cases = evals["evals"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let mut ids = std::collections::HashSet::new();
    for case in cases {
        assert!(ids.insert(case["id"].as_u64().unwrap()));
        assert!(case["prompt"].as_str().unwrap().len() > 100);
        assert!(case["expected_output"].as_str().unwrap().len() > 40);
        assert!(case["files"].as_array().unwrap().is_empty());
    }
}

#[test]
fn profile_workflow_and_competition_are_packaged_with_approval_boundaries() {
    let profile = include_str!("../skills/upwork-profile-management/SKILL.md");
    assert!(profile.contains("name: upwork-profile-management"));
    assert!(profile.contains("For a draft-only request, stop here"));
    assert!(profile.contains("maximum 70 characters"));
    assert!(profile.contains("at most 20"));
    assert!(profile.contains("second approval"));
    let competition = include_str!("../skills/upwork-project-selection/references/competition.md");
    assert!(competition.contains("proposals_tier"));
    assert!(competition.contains("never convert to an exact count"));
    assert!(competition.contains("Limited samples, never complete totals"));
    let evaluations: Value = serde_json::from_str(include_str!(
        "../skills/upwork-profile-management/evals/evals.json"
    ))
    .unwrap();
    assert_eq!(evaluations["skill_name"], "upwork-profile-management");
    let cases = evaluations["evals"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    let mut ids = std::collections::HashSet::new();
    for case in cases {
        assert!(ids.insert(case["id"].as_u64().unwrap()));
        assert!(case["prompt"].as_str().unwrap().len() > 100);
        assert!(case["expected_output"].as_str().unwrap().len() > 40);
        assert!(case["files"].as_array().unwrap().is_empty());
    }
    let allowlist: Value =
        serde_json::from_str(include_str!("../scripts/release-files.json")).unwrap();
    for file in [
        "skills/upwork-profile-management/SKILL.md",
        "skills/upwork-project-selection/references/competition.md",
        "scripts/cleanup.ps1",
    ] {
        assert!(
            allowlist["common"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry == file)
        );
    }
}
