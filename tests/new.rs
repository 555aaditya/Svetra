use std::process::Command;
use svetra::project::TEMPLATES;

fn svetra(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("svetra-new-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_template_loads_clean_and_compiles() {
    for (name, _, body) in TEMPLATES {
        let diags = svetra::report::validate(body);
        assert!(diags.is_empty(), "{name}: {diags:?}");
        svetra::compile::compile_with(body, "svetra.yaml", &std::collections::HashMap::new())
            .unwrap_or_else(|d| panic!("{name} does not compile: {d:?}"));
    }
}

#[test]
fn list_names_each_template_in_plain_words() {
    let out = svetra(&folder("list"), &["new", "--list"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), TEMPLATES.len());
    for (name, about, _) in TEMPLATES {
        assert!(
            stdout
                .lines()
                .any(|l| l.starts_with(name) && l.ends_with(about)),
            "{stdout}"
        );
    }
}

#[test]
fn new_writes_the_template_and_it_compiles() {
    for (name, _, body) in TEMPLATES {
        let dir = folder(name);
        let out = svetra(&dir, &["new", name, "app"]);
        assert_eq!(out.status.code(), Some(0), "{name}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("cd app"), "{stdout}");
        assert_eq!(
            std::fs::read_to_string(dir.join("app/svetra.yaml")).unwrap(),
            body
        );
        let out = svetra(&dir.join("app"), &["compile"]);
        assert_eq!(out.status.code(), Some(0), "{name}");
    }
}

#[test]
fn new_in_the_current_folder() {
    let dir = folder("here");
    let out = svetra(&dir, &["new", "notes"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("cd "));
    assert!(dir.join("svetra.yaml").exists());
}

#[test]
fn new_never_overwrites() {
    let dir = folder("exists");
    std::fs::write(dir.join("svetra.yaml"), "mine").unwrap();
    let out = svetra(&dir, &["new", "blog"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
    assert_eq!(
        std::fs::read_to_string(dir.join("svetra.yaml")).unwrap(),
        "mine"
    );
}

#[test]
fn unknown_template_lists_the_names() {
    let dir = folder("unknown");
    for args in [&["new", "shop"][..], &["new"][..]] {
        let out = svetra(&dir, args);
        assert_eq!(out.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&out.stderr);
        for (name, _, _) in TEMPLATES {
            assert!(stderr.contains(name), "{stderr}");
        }
    }
    assert!(!dir.join("svetra.yaml").exists());
}
