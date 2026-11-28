//! `svetra doctor`'s auth settings (docs/doctor.md, Auth settings). Every project is built at
//! run time in a temporary folder.
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Project {
        let d =
            std::env::temp_dir().join(format!("svetra-doctor-auth-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Project(d)
    }

    fn write(&self, path: &str, text: &str) -> &Project {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
        self
    }

    fn json(&self) -> (i32, Value) {
        let out = Command::new(env!("CARGO_BIN_EXE_svetra"))
            .args(["doctor", self.0.to_str().unwrap(), "--json"])
            .output()
            .unwrap();
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

fn design(auth: &str, roles: &str) -> String {
    format!(
        "svetra: \"0.1\"\nsystem: App\n{auth}{roles}entities:\n  A:\n    private: true\n    fields:\n      id: {{ type: uuid, primary: true }}\n"
    )
}

/// `(kind, object, severity)` of the `auth` findings, in report order.
fn auth(v: &Value) -> Vec<(String, String, String)> {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["area"] == "auth")
        .map(|f| {
            (
                f["kind"].as_str().unwrap().to_string(),
                f["object"].as_str().unwrap().to_string(),
                f["severity"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn f(kind: &str, object: &str, severity: &str) -> (String, String, String) {
    (kind.into(), object.into(), severity.into())
}

fn auth_not_checked(v: &Value) -> Option<String> {
    v["not_checked"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["area"] == "Auth settings")
        .map(|n| n["why"].as_str().unwrap().to_string())
}

#[test]
fn every_risky_sign_in_setting_is_reported_at_its_line() {
    let p = Project::new("risky");
    p.write(
        "supabase/config.toml",
        "[auth]\nminimum_password_length = 6 # short\nenable_signup = true\nenable_anonymous_sign_ins = true\n\n[auth.email]\nenabled = true\nenable_confirmations = false\n",
    );
    let (code, v) = p.json();
    assert_eq!(code, 0, "auth findings never change the exit code");
    assert_eq!(
        auth(&v),
        [
            f("weak_password", "supabase/config.toml:2", "warning"),
            f("anonymous_sign_ins", "supabase/config.toml:4", "warning"),
            f("email_unconfirmed", "supabase/config.toml:8", "warning"),
            f("open_signup", "supabase/config.toml:3", "info"),
        ]
    );
    let msg = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["kind"] == "weak_password")
        .unwrap();
    assert_eq!(msg["message"], "Passwords can be as short as 6 characters.");
    assert!(
        msg["fix"]
            .as_str()
            .unwrap()
            .contains("under Authentication"),
        "{msg}"
    );
    assert!(auth_not_checked(&v).is_none());
}

#[test]
fn missing_keys_have_supabases_defaults() {
    let p = Project::new("defaults");
    p.write("supabase/config.toml", "[auth]\n");
    let (_, v) = p.json();
    assert_eq!(
        auth(&v),
        [
            f("email_unconfirmed", "supabase/config.toml", "warning"),
            f("weak_password", "supabase/config.toml", "warning"),
            f("open_signup", "supabase/config.toml", "info"),
        ]
    );
}

#[test]
fn a_hardened_config_has_no_auth_findings_and_a_length_of_8_is_a_choice() {
    let p = Project::new("hardened");
    p.write(
        "supabase/config.toml",
        "[auth]\nminimum_password_length = 8\nenable_signup = false\nenable_anonymous_sign_ins = false\n[auth.email]\nenable_confirmations = true\n",
    );
    assert_eq!(auth(&p.json().1), []);
}

#[test]
fn a_design_is_checked_before_compile_has_written_a_config() {
    let p = Project::new("design-only");
    p.write(
        "svetra.yaml",
        &design(
            "auth:\n  sign_in: [email]\n  confirm_email: false\n  password: { min_length: 8 }\n",
            "",
        ),
    );
    let (_, v) = p.json();
    let kinds: Vec<String> = auth(&v).into_iter().map(|x| x.0).collect();
    assert_eq!(kinds, ["email_unconfirmed", "open_signup"]);
    assert!(auth(&v).iter().all(|x| x.1.starts_with("svetra.yaml:")));
}

#[test]
fn an_admin_role_without_a_second_factor_is_a_warning() {
    let p = Project::new("admin");
    p.write(
        "svetra.yaml",
        &design(
            "",
            "roles:\n  admin: { description: \"Runs it\" }\n  staff: { description: \"Works\" }\n",
        ),
    );
    let (_, v) = p.json();
    assert_eq!(auth(&v), [f("admin_no_mfa", "svetra.yaml:4", "warning")]);
    let q = Project::new("admin-mfa");
    q.write(
        "svetra.yaml",
        &design(
            "",
            "roles:\n  admin: { description: \"Runs it\", mfa: true }\n",
        ),
    );
    assert_eq!(auth(&q.json().1), []);
}

#[test]
fn without_a_design_who_the_admins_are_is_a_reminder() {
    let p = Project::new("admin-reminder");
    p.write(
        "supabase/config.toml",
        "[auth]\nminimum_password_length = 10\n",
    );
    let (_, v) = p.json();
    let ids: Vec<&str> = v["reminders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"admin_mfa"), "{v}");
}

#[test]
fn a_role_that_needs_mfa_while_authenticator_apps_are_off_is_a_warning() {
    let p = Project::new("mfa-off");
    p.write(
        "svetra.yaml",
        &design(
            "",
            "roles:\n  admin: { description: \"Runs it\", mfa: true }\n",
        ),
    );
    p.write(
        "supabase/config.toml",
        "[auth.mfa.totp]\nenroll_enabled = false\nverify_enabled = false\n",
    );
    let (_, v) = p.json();
    let mfa: Vec<_> = auth(&v).into_iter().filter(|x| x.0 == "mfa_off").collect();
    assert_eq!(mfa, [f("mfa_off", "supabase/config.toml:2", "warning")]);
}

#[test]
fn redirect_urls_that_are_open_or_insecure_are_flagged() {
    let p = Project::new("redirect");
    p.write(
        "supabase/config.toml",
        "[auth]\nsite_url = \"https://app.example.com\"\nadditional_redirect_urls = [\n  \"https://app.example.com/**\",\n  \"https://*\",\n  \"http://staging.example.com\",\n  \"http://localhost:3000\",\n]\nminimum_password_length = 10\nenable_signup = false\n[auth.email]\nenable_confirmations = true\n",
    );
    let (_, v) = p.json();
    let a = auth(&v);
    assert_eq!(
        a,
        [
            f("redirect_open", "supabase/config.toml:3", "warning"),
            f("redirect_open", "supabase/config.toml:3", "warning"),
            f("redirect_open", "supabase/config.toml:3", "warning"),
        ],
        "a wildcard path is fine; a wildcard host, plain http and localhost are not"
    );
    let fixes = v["findings"].to_string();
    assert!(
        !fixes.contains("UNVERIFIED"),
        "the key names are checked now: {fixes}"
    );
    assert!(!fixes.contains("app.example.com/**"), "{fixes}");
}

#[test]
fn local_redirect_urls_with_a_local_site_url_are_not_flagged() {
    let p = Project::new("redirect-local");
    p.write(
        "supabase/config.toml",
        "[auth]\nsite_url = \"http://127.0.0.1:3000\"\nadditional_redirect_urls = [\"https://127.0.0.1:3000\", \"http://localhost:3000\"]\nminimum_password_length = 10\nenable_signup = false\n[auth.email]\nenable_confirmations = true\n",
    );
    assert_eq!(auth(&p.json().1), []);
}

#[test]
fn a_config_doctor_does_not_understand_is_a_note_and_never_a_failure() {
    for text in [
        "[auth]\nthis is not toml\n",
        "[auth\nminimum_password_length = 6\n",
        "[auth]\nminimum_password_length = \"six\"\n",
    ] {
        let p = Project::new("garbled");
        p.write("supabase/config.toml", text);
        let (code, v) = p.json();
        assert_eq!(code, 0, "{text}");
        assert_eq!(auth(&v), [], "{text}");
        let why = auth_not_checked(&v).expect("listed under Not checked");
        assert!(why.contains("was not understood"), "{why}");
        assert_eq!(v["ready"], false);
    }
}

#[test]
fn no_config_and_no_auth_section_is_not_checked_and_says_to_run_supabase_init() {
    let p = Project::new("none");
    p.write("src/main.ts", "console.log(1)\n");
    let (_, v) = p.json();
    assert!(auth_not_checked(&v).unwrap().contains("supabase init"));
}
