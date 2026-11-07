use std::process::Command;

fn svetra(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("svetra-init-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn init_sets_up_a_fresh_folder() {
    let dir = folder("fresh");
    let out = svetra(&dir, &["init"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("added the svetra server to .mcp.json"),
        "{stdout}"
    );
    assert!(stdout.contains("Next: open your agent"));
    let skill = std::fs::read_to_string(dir.join(".claude/skills/svetra/SKILL.md")).unwrap();
    assert!(skill.starts_with("---\nname: svetra\ndescription: "));
    let mcp: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(
        mcp["mcpServers"]["svetra"]["args"],
        serde_json::json!(["-y", "svetra", "mcp"])
    );
    // The starter is valid and compiles.
    let out = svetra(&dir, &["compile"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn init_keeps_other_servers_and_the_design() {
    let dir = folder("keep");
    std::fs::write(
        dir.join(".mcp.json"),
        r#"{"mcpServers": {"zeta": {"command": "zeta"}, "alpha": {"command": "alpha"}}, "other": 1}"#,
    )
    .unwrap();
    std::fs::write(dir.join("svetra.yaml"), "mine\n").unwrap();
    let out = svetra(&dir, &["init"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("kept svetra.yaml"));
    assert_eq!(
        std::fs::read_to_string(dir.join("svetra.yaml")).unwrap(),
        "mine\n"
    );
    let text = std::fs::read_to_string(dir.join(".mcp.json")).unwrap();
    let mcp: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(mcp["other"], 1);
    let names: Vec<&String> = mcp["mcpServers"].as_object().unwrap().keys().collect();
    assert_eq!(
        names,
        ["zeta", "alpha", "svetra"],
        "existing servers keep their order"
    );
    // Running again updates the entry in place.
    let out = svetra(&dir, &["init"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("updated the svetra server"));
}

#[test]
fn init_refuses_broken_mcp_json() {
    let dir = folder("broken");
    std::fs::write(dir.join(".mcp.json"), "{ not json").unwrap();
    let out = svetra(&dir, &["init"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains(".mcp.json isn't valid JSON"));
    assert_eq!(
        std::fs::read_to_string(dir.join(".mcp.json")).unwrap(),
        "{ not json"
    );
}

#[test]
fn the_skills_example_is_valid() {
    let skill = svetra::project::SKILL;
    let start = skill.find("```yaml\n").unwrap() + 8;
    let end = start + skill[start..].find("```").unwrap();
    let diags = svetra::report::validate(&skill[start..end]);
    assert!(!svetra::report::has_errors(&diags), "{diags:#?}");
    assert!(
        diags
            .iter()
            .all(|d| d.severity() != svetra::diag::Severity::Warning),
        "{diags:#?}"
    );
}
