use std::{
    collections::{BTreeMap, HashSet},
    fs,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use chronis::{
    domain::{
        repository::{TaskDetail, TimelineEntry},
        task::Task,
    },
    presentation::toon,
};
use serde_json::{Value, json};
use tempfile::{NamedTempFile, TempDir};

const HEADER: &str = "[id|type|title|pri|status|claimed|blocked_by|parent|archived]";

fn decode_fields(line: &str, delimiter: char, max_splits: usize) -> Vec<String> {
    let mut fields = Vec::new();
    let mut value = String::new();
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => value.push(match chars.next().expect("trailing escape") {
                '\\' => '\\',
                '|' => '|',
                ':' => ':',
                'n' => '\n',
                'r' => '\r',
                other => panic!("unknown escape: {other:?}"),
            }),
            '\n' | '\r' => panic!("unescaped line control"),
            ch if ch == delimiter && fields.len() < max_splits => {
                fields.push(std::mem::take(&mut value));
            }
            ch => value.push(ch),
        }
    }
    fields.push(value);
    fields
}

fn table(output: &str, header: &str, width: usize) -> Vec<Vec<String>> {
    assert!(output.ends_with('\n'));
    let mut lines = output.split_terminator('\n');
    assert_eq!(lines.next(), Some(header));
    lines
        .map(|line| {
            let fields = decode_fields(line, '|', usize::MAX);
            assert_eq!(fields.len(), width, "unexpected row: {line:?}");
            fields
        })
        .collect()
}

fn detail_fields(output: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for line in output.split_terminator('\n') {
        if line == "---" {
            return fields;
        }
        let pair = decode_fields(line, ':', 1);
        assert_eq!(pair.len(), 2, "unexpected detail: {line:?}");
        assert!(fields.insert(pair[0].clone(), pair[1].clone()).is_none());
    }
    panic!("missing detail terminator");
}

fn record(title: &str) -> Task {
    serde_json::from_value(json!({
        "id": "task-a", "title": title, "priority": "p2", "status": "open",
        "blocked_by": [],
    }))
    .unwrap()
}

#[test]
fn ordinary_output_remains_byte_compatible() {
    let mut task = record("Ordinary task");
    task.claimed_by = Some("ci-runner".into());
    task.parent = Some("parent-a".into());
    task.blocked_by = vec!["dep-a".into(), "dep-b".into()];
    task.description = Some("Plain description".into());
    task.done_reason = Some("Verified".into());
    assert_eq!(
        toon::tasks(std::slice::from_ref(&task)),
        format!(
            "{HEADER}\ntask-a|task|Ordinary task|p2|open|ci-runner|dep-a,dep-b|parent-a|false\n"
        )
    );
    assert_eq!(
        toon::task_detail(&TaskDetail {
            task: task.clone(),
            timeline: vec![]
        }),
        "id:task-a\ntype:task\ntitle:Ordinary task\ndescription:Plain description\npriority:p2\nstatus:open\nparent:parent-a\nclaimed_by:ci-runner\nblocked_by:dep-a,dep-b\ndone_reason:Verified\narchived:false\n---\n"
    );
    assert_eq!(
        toon::children(&[task]),
        "[id|status|title]\ntask-a|open|Ordinary task\n"
    );
    assert_eq!(
        toon::timeline(&[TimelineEntry {
            timestamp: "2026-10-27T00:00:00Z".into(),
            event_type: "task.created".into(),
        }]),
        "[timestamp|event_type]\n2026-10-27T00:00:00Z|task.created\n"
    );
    assert_eq!(toon::action("claimed", "task-a"), "ok:claimed:task-a\n");
    assert_eq!(
        toon::created("task", "task-a", "Ordinary task"),
        "created:task:task-a:Ordinary task\n"
    );
    assert_eq!(toon::tasks(&[]), format!("{HEADER}\n"));
    assert!(toon::children(&[]).is_empty());
    assert!(toon::timeline(&[]).is_empty());
    assert_eq!(
        toon::tasks(&[record("")]),
        format!("{HEADER}\ntask-a|task||p2|open||||false\n")
    );
}

#[test]
fn title_escapes_are_injective_and_round_trip() {
    let alphabet = ["", "x", "\\", "|", "\n", "\r", "雪", ":"];
    let mut originals = HashSet::new();
    let mut encodings = HashSet::new();
    for a in alphabet {
        for b in alphabet {
            for c in alphabet {
                let title = format!("{a}{b}{c}");
                if !originals.insert(title.clone()) {
                    continue;
                }
                let encoded = toon::tasks(&[record(&title)]);
                assert_eq!(table(&encoded, HEADER, 9)[0][2], title);
                assert!(
                    encodings.insert(encoded),
                    "colliding encoding for {title:?}"
                );
            }
        }
    }
    assert_eq!(
        toon::tasks(&[record("\\n|\n\r\\")]),
        format!("{HEADER}\ntask-a|task|\\\\n\\|\\n\\r\\\\|p2|open||||false\n")
    );
}

#[test]
fn every_string_value_uses_same_escape_contract_without_mutating_state() {
    let value = "value:part\\n\\r\\|\\\\|\n\r雪";
    let mut task = record(value);
    task.id = value.into();
    task.parent = Some(value.into());
    task.claimed_by = Some(value.into());
    task.description = Some(value.into());
    task.done_reason = Some(value.into());
    task.blocked_by = vec![value.into(), "dep-b".into()];
    let before = serde_json::to_value(&task).unwrap();
    let row = table(&toon::tasks(std::slice::from_ref(&task)), HEADER, 9).remove(0);
    for index in [0, 2, 5, 7] {
        assert_eq!(row[index], value);
    }
    assert_eq!(row[6], format!("{value},dep-b"));
    let detail = TaskDetail {
        task,
        timeline: vec![],
    };
    let fields = detail_fields(&toon::task_detail(&detail));
    assert_eq!(fields.len(), 11);
    for key in [
        "id",
        "title",
        "parent",
        "claimed_by",
        "description",
        "done_reason",
    ] {
        assert_eq!(fields[key], value);
    }
    assert_eq!(fields["blocked_by"], format!("{value},dep-b"));
    assert_eq!(fields["status"], "open");
    let rows = table(
        &toon::children(std::slice::from_ref(&detail.task)),
        "[id|status|title]",
        3,
    );
    assert_eq!(rows[0], [value, "open", value]);
    let rows = table(
        &toon::timeline(&[TimelineEntry {
            timestamp: value.into(),
            event_type: value.into(),
        }]),
        "[timestamp|event_type]",
        2,
    );
    assert_eq!(rows[0], [value, value]);
    assert_eq!(
        decode_fields(
            toon::action(value, value).strip_suffix('\n').unwrap(),
            ':',
            2
        ),
        ["ok", value, value]
    );
    assert_eq!(
        decode_fields(
            toon::created(value, value, value)
                .strip_suffix('\n')
                .unwrap(),
            ':',
            3
        ),
        ["created", value, value, value]
    );
    assert_eq!(serde_json::to_value(&detail.task).unwrap(), before);
}

// The guard owns each child through timeout and unwinding; kill must be followed by reap.
struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct CliWorkspace {
    dir: TempDir,
    deadline: Instant,
}

impl CliWorkspace {
    fn new() -> Self {
        let workspace = Self {
            dir: tempfile::tempdir().unwrap(),
            deadline: Instant::now() + Duration::from_secs(120),
        };
        workspace.ok(&["init"], "ci-runner");
        assert!(workspace.dir.path().join(".chronis/config.toml").is_file());
        workspace
    }

    fn run(&self, args: &[&str], agent: &str) -> (ExitStatus, String, String) {
        assert!(
            Instant::now() < self.deadline,
            "lifecycle deadline exceeded"
        );
        let stdout = NamedTempFile::new_in(self.dir.path()).unwrap();
        let stderr = NamedTempFile::new_in(self.dir.path()).unwrap();
        let mut child = OwnedChild(
            Command::new(env!("CARGO_BIN_EXE_cn"))
                .args(["--toon", "--timeout", "15"])
                .args(args)
                .current_dir(self.dir.path())
                .env_clear()
                .env("CN_AGENT_ID", agent)
                .env("RUST_LOG", "off")
                .stdin(Stdio::null())
                .stdout(stdout.reopen().unwrap())
                .stderr(stderr.reopen().unwrap())
                .spawn()
                .expect("spawn source-built cn"),
        );
        let deadline = self.deadline.min(Instant::now() + Duration::from_secs(20));
        let status = loop {
            if let Some(status) = child.0.try_wait().expect("poll cn") {
                break status;
            }
            if Instant::now() >= deadline {
                child.0.kill().expect("kill timed-out cn");
                child.0.wait().expect("reap timed-out cn");
                panic!(
                    "cn {args:?} exceeded deadline; stderr: {}",
                    fs::read_to_string(stderr.path()).unwrap()
                );
            }
            thread::sleep(Duration::from_millis(10));
        };
        (
            status,
            fs::read_to_string(stdout.path()).unwrap(),
            fs::read_to_string(stderr.path()).unwrap(),
        )
    }

    fn ok(&self, args: &[&str], agent: &str) -> String {
        let (status, stdout, stderr) = self.run(args, agent);
        assert!(
            status.success(),
            "cn {args:?}: {status}; stdout: {stdout}; stderr: {stderr}"
        );
        assert!(
            stderr.is_empty(),
            "cn {args:?}: unexpected stderr: {stderr}"
        );
        stdout
    }

    fn fails(&self, args: &[&str], error: &str) {
        let (status, stdout, stderr) = self.run(args, "other-agent");
        assert_eq!(status.code(), Some(1), "cn {args:?}: {stdout}; {stderr}");
        assert!(
            stdout.is_empty(),
            "failure emitted success output: {stdout}"
        );
        assert!(stderr.contains(error), "expected {error:?}, got {stderr:?}");
    }

    fn json(&self) -> Value {
        let mut tasks: Vec<Value> =
            serde_json::from_str(&self.ok(&["list", "--all", "--format", "json"], "ci-runner"))
                .unwrap();
        tasks.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Value::Array(tasks)
    }
}

#[test]
fn cli_lifecycle_preserves_free_text_across_processes_and_rejects_invalid_transitions() {
    let cli = CliWorkspace::new();
    let titles = [
        "Ordinary task",
        "literal\\n",
        "literal\n",
        "literal\\r",
        "literal\r",
        "pipe|slash\\\r\n雪",
        "literal:colon",
    ];
    let description = "description|\\n\nstatus:done\r雪";
    let agent = "agent|column\\n\nstatus:done\rrow";
    let reason = "reason|\\n\nstatus:open\r雪";
    let mut ids = Vec::new();
    for title in titles {
        let output = cli.ok(&["task", "create", title, "-d", description], "ci-runner");
        let fields = decode_fields(output.strip_suffix('\n').unwrap(), ':', 3);
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0], "created");
        assert_eq!(fields[1], "task");
        assert_eq!(fields[3], title);
        ids.push(fields[2].clone());
    }
    let id = &ids[5];
    cli.fails(&["release", id], "is not claimed; nothing to release");
    assert_eq!(cli.ok(&["claim", id], agent), format!("ok:claimed:{id}\n"));
    cli.fails(&["claim", id], "is already claimed by");
    let before_reads = cli.json();
    let output = cli.ok(&["list"], "ci-runner");
    let rows = table(&output, HEADER, 9);
    assert_eq!(rows.len(), titles.len());
    for (index, title) in titles.iter().enumerate() {
        let row = rows.iter().find(|row| row[0] == ids[index]).unwrap();
        assert_eq!(&row[2], title);
        assert_eq!(row[4], if index == 5 { "in-progress" } else { "open" });
        assert_eq!(row[5], if index == 5 { agent } else { "" });
        let shown = cli.ok(&["show", &ids[index]], "ci-runner");
        let fields = detail_fields(&shown);
        assert_eq!(&fields["title"], title);
        assert_eq!(fields["description"], description);
        assert_eq!(fields["status"], row[4]);
        assert_eq!(fields.len(), if index == 5 { 8 } else { 7 });
        if index == 5 {
            assert_eq!(fields["claimed_by"], agent);
            assert_eq!(shown.matches("\nstatus:").count(), 1);
        }
    }
    assert!(output.contains(&format!("{}|task|Ordinary task|p2|open||||false\n", ids[0])));
    let stored = before_reads.as_array().unwrap();
    assert_eq!(stored.len(), titles.len());
    for (index, title) in titles.iter().enumerate() {
        let task = stored.iter().find(|task| task["id"] == ids[index]).unwrap();
        assert_eq!(task["title"].as_str(), Some(*title));
        assert_eq!(task["description"], description);
        if index == 5 {
            assert_eq!(task["claimed_by"], agent);
        }
    }
    assert_eq!(
        cli.json(),
        before_reads,
        "TOON reads changed persisted state"
    );
    assert_eq!(
        cli.ok(&["done", id, "--reason", reason], "ci-runner"),
        format!("ok:done:{id}\n")
    );
    let completed = cli.json();
    let task = completed
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == *id)
        .unwrap();
    assert_eq!(task["status"], "done");
    assert_eq!(task["title"], titles[5]);
    assert_eq!(task["description"], description);
    assert_eq!(task["claimed_by"], agent);
    assert_eq!(task["done_reason"], reason);
    let fields = detail_fields(&cli.ok(&["show", id], "ci-runner"));
    assert_eq!(fields["status"], "done");
    assert_eq!(fields["done_reason"], reason);
    assert!(cli.ok(&["done", id], "ci-runner").is_empty());
    cli.fails(&["claim", id], "can only claim open tasks");
    assert_eq!(cli.json(), completed, "rejected transitions changed state");
}
