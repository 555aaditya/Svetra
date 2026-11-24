//! `svetra doctor`'s folder checks (docs/doctor.md). Every project is built at run time in a
//! temporary folder, and every secret-shaped string is assembled from pieces, so nothing
//! key-like is committed to this repository.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn svetra(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(args)
        .output()
        .unwrap()
}

fn base64url(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// A JWT with a dummy signature, the way a Supabase key is shaped.
fn jwt(claims: &str) -> String {
    format!(
        "{}.{}.{}",
        base64url(br#"{"alg":"HS256","typ":"JWT"}"#),
        base64url(claims.as_bytes()),
        base64url(b"dummy-signature-not-a-real-one")
    )
}

fn service_key() -> String {
    jwt(r#"{"iss":"supabase","ref":"abcdefghijklmnop","role":"service_role","exp":1983812996}"#)
}

fn anon_key() -> String {
    jwt(r#"{"iss":"supabase","ref":"abcdefghijklmnop","role":"anon","exp":1983812996}"#)
}

/// A project folder in a fresh temporary directory.
struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Project {
        let d = std::env::temp_dir().join(format!("svetra-doctor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Project(d)
    }

    fn git() -> impl Fn(&Path, &[&str]) {
        |dir, args| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        }
    }

    /// `git init` and nothing else: no files are tracked.
    fn init(self) -> Project {
        Project::git()(&self.0, &["init", "-q"]);
        self
    }

    fn write(&self, path: &str, text: &str) -> &Project {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
        self
    }

    fn track(&self, path: &str) -> &Project {
        Project::git()(&self.0, &["add", "-f", path]);
        self
    }

    fn doctor(&self) -> Output {
        svetra(&["doctor", self.0.to_str().unwrap()])
    }

    fn json(&self) -> (i32, Value) {
        let out = svetra(&["doctor", self.0.to_str().unwrap(), "--json"]);
        (
            out.status.code().unwrap(),
            serde_json::from_slice(&out.stdout).unwrap(),
        )
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `(kind, object, severity)` of every finding outside `auth`, which has its own tests: a
/// project with a `config.toml` and no `auth:` section always gets Supabase's defaults.
fn found(v: &Value) -> Vec<(String, String, String)> {
    found_in(v, |a| a != "auth")
}

fn found_in(v: &Value, keep: impl Fn(&str) -> bool) -> Vec<(String, String, String)> {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| keep(f["area"].as_str().unwrap()))
        .map(|f| {
            (
                f["kind"].as_str().unwrap().to_string(),
                f["object"].as_str().unwrap().to_string(),
                f["severity"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn finding(kind: &str, object: &str, severity: &str) -> (String, String, String) {
    (kind.to_string(), object.to_string(), severity.to_string())
}

/// Neither output holds the secret, any 8-character slice of it, or its length.
fn assert_no_secret(p: &Project, secrets: &[&str]) {
    let human = String::from_utf8(p.doctor().stdout).unwrap();
    let json =
        String::from_utf8(svetra(&["doctor", p.0.to_str().unwrap(), "--json"]).stdout).unwrap();
    for secret in secrets {
        for (name, text) in [("human", &human), ("json", &json)] {
            for w in secret.as_bytes().windows(8) {
                let slice = std::str::from_utf8(w).unwrap();
                assert!(
                    !text.contains(slice),
                    "{name} output holds a slice of a secret"
                );
            }
            let len = secret.len();
            for words in [
                format!("{len} characters"),
                format!("{len} chars"),
                format!("length {len}"),
            ] {
                assert!(!text.contains(&words), "{name} output holds a length");
            }
        }
    }
}

#[test]
fn a_service_key_in_front_end_code_is_critical_and_never_printed() {
    let key = service_key();
    let p = Project::new("service").init();
    p.write(
        "src/lib/supabase.ts",
        &format!("import {{ createClient }} from '@supabase/supabase-js'\n\nconst url = 'https://x.supabase.co'\nconst key = '{key}'\nexport const db = createClient(url, key)\n"),
    );
    let (code, v) = p.json();
    assert_eq!(code, 1);
    assert_eq!(
        found(&v),
        [finding(
            "service_key_exposed",
            "src/lib/supabase.ts:4",
            "critical"
        )]
    );
    assert_eq!(v["findings"][0]["area"], "keys");
    assert_eq!(v["findings"][0]["line"], 4);
    assert!(
        v["findings"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("rotate it"),
        "{v}"
    );
    assert!(
        v["findings"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("server function")
    );
    assert_eq!(v["counts"]["critical"], 1);
    assert_eq!(v["ready"], false);
    let human = String::from_utf8(p.doctor().stdout).unwrap();
    assert!(human.contains("critical  src/lib/supabase.ts:4"), "{human}");
    assert!(human.contains("Not ready."), "{human}");
    assert_no_secret(&p, &[&key]);
}

#[test]
fn the_anon_and_publishable_keys_are_never_flagged() {
    let anon = anon_key();
    let publishable = format!("sb_publishable_{}", "AbCdEf0123456789xYzQrStUv");
    let p = Project::new("anon").init();
    p.write(
        "src/lib/supabase.ts",
        &format!("const a = '{anon}'\nconst b = '{publishable}'\n"),
    );
    p.write(
        ".env",
        &format!("NEXT_PUBLIC_SUPABASE_URL=https://x.supabase.co\nNEXT_PUBLIC_SUPABASE_ANON_KEY={anon}\nSUPABASE_SERVICE_ROLE_KEY={anon}\nPUBLISHABLE_KEY={publishable}\n"),
    );
    p.track(".env");
    p.write(
        "supabase/config.toml",
        &format!("[auth]\nsite_url = \"http://localhost:3000\"\n# anon_key = \"{anon}\"\n"),
    );
    p.track("supabase/config.toml");
    let (code, v) = p.json();
    assert_eq!(code, 0, "{v}");
    assert_eq!(found(&v), []);
    assert_eq!(v["ready"], false, "the database was not checked");
}

#[test]
fn a_service_key_in_an_ignored_server_side_env_file_is_where_it_belongs() {
    let key = service_key();
    let p = Project::new("ignored").init();
    p.write(".gitignore", ".env*\n!.env.example\n");
    p.write(".env", &format!("SUPABASE_SERVICE_ROLE_KEY={key}\n"));
    let (code, v) = p.json();
    assert_eq!(code, 0, "{v}");
    assert_eq!(found(&v), []);
}

#[test]
fn a_public_variable_name_puts_even_an_ignored_key_in_the_browser() {
    let key = service_key();
    let p = Project::new("public-name").init();
    p.write(".gitignore", ".env*\n");
    p.write(".env.local", &format!("NEXT_PUBLIC_SUPABASE_KEY={key}\n"));
    let (code, v) = p.json();
    assert_eq!(code, 1);
    assert_eq!(
        found(&v),
        [finding("service_key_exposed", ".env.local:1", "critical")]
    );
    assert_no_secret(&p, &[&key]);
}

#[test]
fn a_key_is_judged_by_what_it_decodes_to_not_by_its_variable_name() {
    let key = service_key();
    let p = Project::new("name").init();
    p.write("src/app.ts", &format!("const SUPABASE_KEY = '{key}'\n"));
    let (_, v) = p.json();
    assert_eq!(
        found(&v),
        [finding("service_key_exposed", "src/app.ts:1", "critical")]
    );
}

#[test]
fn a_new_style_secret_key_is_critical_in_front_end_code() {
    let key = format!("sb_secret_{}", "AbCdEf0123456789xYzQrStUv");
    let p = Project::new("sb-secret").init();
    p.write("src/app.ts", &format!("const k = '{key}'\n"));
    let (code, v) = p.json();
    assert_eq!(code, 1);
    assert_eq!(
        found(&v),
        [finding("service_key_exposed", "src/app.ts:1", "critical")]
    );
    assert_no_secret(&p, &[&key]);
}

#[test]
fn a_service_key_in_a_tracked_example_file_is_a_leak() {
    let key = service_key();
    let p = Project::new("example").init();
    p.write(".env.example", &format!("KEY={key}\n"));
    p.track(".env.example");
    let (_, v) = p.json();
    assert_eq!(
        found(&v),
        [finding("service_key_exposed", ".env.example:1", "critical")],
        "an example file is committed on purpose, so a real key there is a leak"
    );
}

#[test]
fn a_tracked_env_file_with_a_secret_is_critical() {
    let key = service_key();
    let p = Project::new("env-secret").init();
    p.write(
        ".env.production",
        &format!("SUPABASE_URL=https://x.supabase.co\nSUPABASE_SERVICE_ROLE_KEY={key}\nSTRIPE_TOKEN=hunter2hunter2\n"),
    );
    p.track(".env.production");
    let (code, v) = p.json();
    assert_eq!(code, 1);
    assert_eq!(
        found(&v),
        [
            finding("env_committed", ".env.production", "critical"),
            finding("service_key_exposed", ".env.production:2", "critical"),
        ]
    );
    let msg = v["findings"][0]["message"].as_str().unwrap();
    assert!(
        msg.contains("2 values that aren't public by design"),
        "{msg}"
    );
    assert_no_secret(&p, &[&key, "hunter2hunter2"]);
}

#[test]
fn a_tracked_env_file_with_only_a_non_public_value_is_a_warning() {
    let p = Project::new("env-warn").init();
    p.write(
        ".env",
        "SUPABASE_URL=https://x.supabase.co\nSMTP_PASSWORD=hunter2hunter2\nEMPTY_TOKEN=\n",
    );
    p.track(".env");
    let (code, v) = p.json();
    assert_eq!(code, 0, "a warning doesn't change the exit code");
    assert_eq!(found(&v), [finding("env_committed", ".env", "warning")]);
    assert_no_secret(&p, &["hunter2hunter2"]);
}

#[test]
fn a_tracked_env_file_with_nothing_secret_in_it_is_not_reported() {
    let p = Project::new("env-clean").init();
    p.write(".env", "SUPABASE_URL=https://x.supabase.co\nPORT=3000\n");
    p.track(".env");
    let (_, v) = p.json();
    assert_eq!(found(&v), []);
}

#[test]
fn an_env_file_that_is_neither_tracked_nor_ignored_is_a_warning() {
    let p = Project::new("not-ignored").init();
    p.write(".env.local", "PORT=3000\n");
    p.write(".env.example", "PORT=3000\n");
    let (code, v) = p.json();
    assert_eq!(code, 0);
    assert_eq!(
        found(&v),
        [finding("env_not_ignored", ".env.local", "warning")]
    );
}

#[test]
fn a_literal_secret_in_config_toml_is_reported_and_an_env_reference_is_not() {
    let p = Project::new("config").init();
    p.write(
        "supabase/config.toml",
        "[auth.external.google]\nenabled = true\nclient_id = \"abc.apps.googleusercontent.com\"\nsecret = \"GOCSPX-hunter2hunter2\"\n\n[auth.external.github]\nsecret = \"env(SUPABASE_AUTH_EXTERNAL_GITHUB_SECRET)\"\n\n[auth]\nminimum_password_length = 6\npassword_requirements = \"\"\n",
    );
    p.track("supabase/config.toml");
    let (code, v) = p.json();
    assert_eq!(code, 1, "tracked: critical");
    assert_eq!(
        found(&v),
        [finding(
            "config_secret",
            "supabase/config.toml:4",
            "critical"
        )]
    );
    assert_no_secret(&p, &["GOCSPX-hunter2hunter2"]);

    // Untracked and ignored, it is a warning.
    let q = Project::new("config-ignored").init();
    q.write(".gitignore", "supabase/config.toml\n");
    q.write(
        "supabase/config.toml",
        "[auth.external.google]\nsecret = \"hunter2hunter2\"\n",
    );
    let (code, v) = q.json();
    assert_eq!(code, 0);
    assert_eq!(
        found(&v),
        [finding(
            "config_secret",
            "supabase/config.toml:2",
            "warning"
        )]
    );
}

#[test]
fn a_jwt_secret_is_critical_in_config_toml_and_in_a_tracked_env_file() {
    let secret = "q9XvLm2pTz7RbN4kYw8cHd3sJf6aGe1u";
    let p = Project::new("jwt-secret").init();
    p.write(
        "supabase/config.toml",
        &format!("[auth]\njwt_secret = \"{secret}\"\n"),
    );
    p.write("server.env.txt", &format!("SUPABASE_JWT_SECRET={secret}\n"));
    p.write(
        "supabase/other.toml",
        "jwt_secret = \"env(SUPABASE_JWT_SECRET)\"\n",
    );
    p.track("supabase/config.toml");
    p.track("server.env.txt");
    p.track("supabase/other.toml");
    let (code, v) = p.json();
    assert_eq!(code, 1);
    assert_eq!(
        found(&v),
        [
            finding("jwt_secret_exposed", "server.env.txt:1", "critical"),
            finding("jwt_secret_exposed", "supabase/config.toml:2", "critical"),
        ]
    );
    assert_no_secret(&p, &[secret]);
}

#[test]
fn a_database_password_in_a_connection_string_is_critical() {
    let password = "t0pS3cretPw9z";
    let p = Project::new("db").init();
    p.write(
        ".env",
        &format!(
            "DATABASE_URL=postgresql://postgres:{password}@db.abcdefgh.supabase.co:5432/postgres\nLOCAL=postgresql://postgres:postgres@127.0.0.1:54322/postgres\nTEMPLATE=postgresql://postgres:[YOUR-PASSWORD]@db.abcdefgh.supabase.co:5432/postgres\n"
        ),
    );
    p.track(".env");
    let (code, v) = p.json();
    assert_eq!(code, 1);
    assert_eq!(
        found(&v),
        [
            finding("env_committed", ".env", "critical"),
            finding("db_password_exposed", ".env:1", "critical"),
        ]
    );
    assert_no_secret(&p, &[password]);
}

#[test]
fn other_secrets_are_reported_by_their_format() {
    let live = format!("sk_live_{}", "4eC39HqLyjWDarjtT1zdp7dc");
    let test = format!("sk_test_{}", "4eC39HqLyjWDarjtT1zdp7dc");
    let pat = format!("ghp_{}", "aB3dE5gH7jK9mN1pQ3sT5vW7yZ9bD1fH3jK5");
    let begin = format!("-----BEGIN {} KEY-----", "RSA PRIVATE");
    let p = Project::new("vendors").init();
    p.write(
        "server/pay.ts",
        &format!("const live = '{live}'\nconst test = '{test}'\nconst gh = '{pat}'\nconst sample = 'sk_live_your_key_here_xxxxxxxxxxxx'\n// a header looks like -----BEGIN ... PRIVATE KEY-----\n"),
    );
    p.write("deploy.pem", &format!("{begin}\nMIIEvQIBADANBg\n"));
    p.track("server/pay.ts");
    p.track("deploy.pem");
    let (code, v) = p.json();
    assert_eq!(code, 1);
    assert_eq!(
        found(&v),
        [
            finding("secret_exposed", "deploy.pem:1", "critical"),
            finding("secret_exposed", "server/pay.ts:1", "critical"),
            finding("secret_exposed", "server/pay.ts:3", "critical"),
            finding("secret_exposed", "server/pay.ts:2", "warning"),
        ]
    );
    assert_no_secret(&p, &[&live, &test, &pat]);
}

#[test]
fn a_jwt_with_an_unrecognised_role_is_a_note_once_per_file() {
    let session = jwt(r#"{"sub":"2d6f","role":"authenticated","exp":1983812996}"#);
    let p = Project::new("unrecognised").init();
    p.write(
        "docs/notes.md",
        &format!("a\nb\n{session}\nand again {session}\n"),
    );
    p.write("tests/session.test.ts", &format!("const t = '{session}'\n"));
    p.write("notes-untracked.md", &format!("{session}\n"));
    p.track("docs/notes.md");
    let (code, v) = p.json();
    assert_eq!(code, 0, "an info never changes the exit code");
    assert_eq!(
        found(&v),
        [finding("jwt_unrecognized", "docs/notes.md:3", "info")]
    );
    assert_eq!(v["counts"]["info"], 1);
    assert_no_secret(&p, &[&session]);
}

#[test]
fn a_folder_that_is_not_a_git_repository_treats_every_secret_as_tracked() {
    let key = service_key();
    let p = Project::new("no-git");
    p.write(".env", &format!("SUPABASE_SERVICE_ROLE_KEY={key}\n"));
    let (code, v) = p.json();
    assert_eq!(code, 1);
    assert_eq!(
        found(&v),
        [finding("service_key_exposed", ".env:1", "critical")]
    );
    let not_checked: Vec<&str> = v["not_checked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["area"].as_str().unwrap())
        .collect();
    assert!(not_checked.contains(&"Git"), "{v}");
    assert_no_secret(&p, &[&key]);
}

#[test]
fn a_clean_folder_is_never_called_ready_without_a_database() {
    let p = Project::new("clean").init();
    p.write("src/main.ts", "console.log('hi')\n");
    let out = p.doctor();
    assert_eq!(out.status.code(), Some(0));
    let human = String::from_utf8(out.stdout).unwrap();
    let first = human.lines().next().unwrap();
    assert!(first.contains("database not checked"), "{human}");
    assert!(human.contains("Database: not checked"), "{human}");
    let last = human.lines().last().unwrap();
    assert!(last.contains("Not a clearance."), "{human}");
    assert!(!last.contains("The reminders above are yours"), "{human}");
    let (_, v) = p.json();
    assert_eq!(v["ready"], false);
    assert_eq!(v["counts"]["critical"], 0);
}

#[test]
fn backups_are_a_reminder_when_the_project_looks_hosted() {
    let p = Project::new("reminders").init();
    p.write("supabase/config.toml", "[auth]\n");
    let (_, v) = p.json();
    let ids: Vec<&str> = v["reminders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["backups", "restore", "own_copy", "admin_mfa"]);
    assert_eq!(found(&v), [], "reminders are never findings");
    let q = Project::new("no-reminders").init();
    assert_eq!(q.json().1["reminders"], serde_json::json!([]));
}

#[test]
fn generated_files_that_drifted_are_a_warning_per_file() {
    let p = Project::new("stale").init();
    p.write("svetra.yaml", include_str!("fixtures/posts.yaml"));
    let compile = svetra(&["compile", p.0.join("svetra.yaml").to_str().unwrap()]);
    assert!(compile.status.success());
    let (code, v) = p.json();
    assert_eq!(code, 0);
    assert_eq!(found(&v), [], "freshly compiled files are current");
    assert!(
        !v["not_checked"].to_string().contains("Generated files"),
        "{v}"
    );

    let sql = p.0.join("supabase/schemas/svetra.sql");
    let edited = format!("{}\n-- by hand\n", std::fs::read_to_string(&sql).unwrap());
    std::fs::write(&sql, edited).unwrap();
    let before = std::fs::read_to_string(&sql).unwrap();
    let (code, v) = p.json();
    assert_eq!(code, 0, "a warning never changes the exit code");
    assert_eq!(
        found(&v),
        [finding(
            "stale_generated",
            "supabase/schemas/svetra.sql",
            "warning"
        )]
    );
    assert_eq!(v["findings"][0]["area"], "generated");
    assert_eq!(
        std::fs::read_to_string(&sql).unwrap(),
        before,
        "doctor never writes"
    );
}

#[test]
fn an_invalid_design_is_a_warning_that_says_what_could_not_be_checked() {
    let p = Project::new("invalid").init();
    p.write("svetra.yaml", "entities: nonsense\n");
    let (code, v) = p.json();
    assert_eq!(code, 0);
    assert_eq!(
        found(&v),
        [finding("design_invalid", "svetra.yaml", "warning")]
    );
    assert!(
        v["findings"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("svetra validate")
    );
}

#[test]
fn a_folder_without_a_design_lists_generated_files_as_not_checked() {
    let p = Project::new("no-design").init();
    let (_, v) = p.json();
    assert!(v["not_checked"].to_string().contains("Generated files"));
}

#[test]
fn a_symbolic_link_out_of_the_folder_is_not_followed() {
    let key = service_key();
    let outside = Project::new("outside");
    outside.write("leak.ts", &format!("const k = '{key}'\n"));
    let p = Project::new("link").init();
    p.write("src/a.ts", "export {}\n");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.0.join("leak.ts"), p.0.join("src/leak.ts")).unwrap();
        std::os::unix::fs::symlink(&outside.0, p.0.join("linked")).unwrap();
    }
    let (code, v) = p.json();
    assert_eq!(code, 0);
    assert_eq!(found(&v), []);
}

#[test]
fn output_is_the_same_every_run() {
    let key = service_key();
    let p = Project::new("determinism").init();
    p.write("b.ts", &format!("k = '{key}'\n"));
    p.write("a.ts", &format!("k = '{key}'\n"));
    p.write(".env", "TOKEN=hunter2hunter2\n");
    let first = p.doctor().stdout;
    assert_eq!(first, p.doctor().stdout);
    let human = String::from_utf8(first).unwrap();
    assert!(human.find("a.ts:1").unwrap() < human.find("b.ts:1").unwrap());
}

#[test]
fn usage_errors_exit_2() {
    let out = svetra(&["doctor", "/no/such/folder/anywhere"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not a folder"));
    let file = Project::new("a-file");
    file.write("f.txt", "x");
    let out = svetra(&["doctor", file.0.join("f.txt").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    for flag in ["--db", "--bogus"] {
        let out = svetra(&["doctor", flag]);
        assert_eq!(out.status.code(), Some(2), "{flag}");
    }
    assert_eq!(svetra(&["doctor", "a", "b"]).status.code(), Some(2));
}
