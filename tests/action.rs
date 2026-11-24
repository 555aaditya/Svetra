//! `action.yml` (docs/action.md): the documented inputs and outputs exist, and no GitHub
//! expression is pasted into a shell script, where a branch name or an input could inject commands.

const ACTION: &str = include_str!("../action.yml");

#[test]
fn declares_the_documented_inputs_and_outputs() {
    for input in ["file", "base", "accept", "version", "comment", "token"] {
        assert!(
            ACTION.contains(&format!("\n  {input}:\n    description:")),
            "missing input {input}"
        );
    }
    for output in ["needs_yes", "changes"] {
        assert!(
            ACTION.contains(&format!("\n  {output}:\n    description:")),
            "missing output {output}"
        );
        assert!(ACTION.contains(&format!("steps.plan.outputs.{output}")));
    }
    assert!(ACTION.contains("using: composite"));
}

#[test]
fn no_expression_inside_a_run_script() {
    let (mut in_script, mut run_indent) = (false, 0);
    for (n, line) in ACTION.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        if line.trim_end().ends_with("run: |") {
            in_script = true;
            run_indent = indent;
            continue;
        }
        // A script ends at the first line that is indented no more than its `run:` key.
        if in_script && !line.trim().is_empty() && indent <= run_indent {
            in_script = false;
        }
        assert!(
            !(in_script && line.contains("${{")),
            "line {}: an expression inside a script: {line}",
            n + 1
        );
    }
}

#[test]
fn never_runs_with_the_pull_request_target_event_in_its_docs() {
    let docs = include_str!("../docs/action.md");
    assert!(docs.contains("pull_request:"));
    assert!(!docs.contains("on:\n  pull_request_target"));
}

#[test]
fn comments_with_one_marker_and_edits_it_in_place() {
    assert!(ACTION.contains("<!-- svetra-plan -->"));
    assert!(ACTION.contains("-X PATCH"));
}
