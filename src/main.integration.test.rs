use super::*;
use crate::mock_jev::{MockJev, Request};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static FOLDERS: AtomicUsize = AtomicUsize::new(0);

struct Folder {
    path: PathBuf,
}

impl Folder {
    fn new() -> Folder {
        let path = std::env::temp_dir().join(format!(
            "pyes-integration-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::SeqCst)
        ));

        std::fs::create_dir_all(&path).unwrap();

        Folder { path }
    }

    fn environment_of(&self, api_key: Option<&str>, mock: &MockJev) -> Environment {
        let folder = Some(self.path.to_string_lossy().into_owned());

        Environment {
            api_key: api_key.map(str::to_string),
            base_url: Some(mock.url.clone()),
            appdata: folder.clone(),
            xdg_config_home: folder.clone(),
            home: folder,
        }
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct Outcome {
    stdout: Vec<u8>,
    stderr: String,
    code: i32,
}

fn outcome_of(arguments: &[&str], input: &[u8], environment: Environment) -> Outcome {
    let arguments: Vec<OsString> = std::iter::once("pyes")
        .chain(arguments.iter().copied())
        .map(OsString::from)
        .collect();
    let input = input.to_vec();
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        let mut stdin = input.as_slice();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            arguments,
            &environment,
            &mut Streams {
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
            },
        );

        let _ = sender.send(Outcome {
            stdout,
            stderr: String::from_utf8(stderr).unwrap(),
            code,
        });
    });

    receiver
        .recv_timeout(Duration::from_secs(60))
        .expect("run did not return within 60 s")
}

fn question_of(instructions: &str) -> &str {
    instructions.split_once("`, ").unwrap().1
}

fn answered_of(body: &Value, noul_of: impl Fn(&str, &str) -> f64) -> String {
    let mut answers = Map::new();

    for (id, question) in body["questions"].as_object().unwrap() {
        let line = body["state"]["lines"][id].as_str().unwrap();
        let instructions = question["instructions"].as_str().unwrap();

        answers.insert(
            id.clone(),
            json!({ "type": "noul", "noul": noul_of(question_of(instructions), line) }),
        );
    }

    json!({ "model": "jev-1.13.0", "answers": answers, "usage": { "input_tokens": 1 } }).to_string()
}

fn ok_of(body: String) -> (u16, Vec<(String, String)>, String) {
    (200, Vec::new(), body)
}

fn half(request: &Request) -> (u16, Vec<(String, String)>, String) {
    ok_of(answered_of(&request.body, |_, _| 0.5))
}

fn text_of(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn scores_two_questions_over_three_lines_in_input_order() {
    let folder = Folder::new();
    let mock = MockJev::start(|request| {
        ok_of(answered_of(&request.body, |question, line| {
            match (question.starts_with("is it first"), line) {
                (true, "one") => 0.1,
                (true, "two") => 0.2,
                (true, _) => 0.3,
                (false, "one") => 0.6,
                (false, "two") => 0.7,
                (false, _) => 0.8,
            }
        }))
    });
    let outcome = outcome_of(
        &["Is it first?", "Is it second?"],
        b"one\ntwo\nthree\n",
        folder.environment_of(Some("test"), &mock),
    );

    assert_eq!(
        (text_of(&outcome.stdout), outcome.stderr, outcome.code),
        (
            "0.1\t0.6\tone\n0.2\t0.7\ttwo\n0.3\t0.8\tthree\n".to_string(),
            String::new(),
            SUCCESS
        )
    );

    let requests = mock.requests.lock().unwrap();

    assert_eq!(requests.len(), 2);

    for request in requests.iter() {
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("POST", "/v1/systemone")
        );
        assert_eq!(request.header_of("authorization"), Some("Bearer test"));
        assert_eq!(
            request.header_of("content-length"),
            Some(
                serde_json::to_vec(&request.body)
                    .unwrap()
                    .len()
                    .to_string()
                    .as_str()
            )
        );
    }
}

#[test]
fn nul_records_round_trip_with_nul_terminators() {
    let folder = Folder::new();
    let mock = MockJev::start(half);
    let outcome = outcome_of(
        &["-z", "Is this a?"],
        b"a\nb\0c\0",
        folder.environment_of(Some("test"), &mock),
    );

    assert_eq!(
        (outcome.stdout, outcome.code),
        (b"0.5\ta\nb\x000.5\tc\0".to_vec(), SUCCESS)
    );
    assert_eq!(
        mock.requests.lock().unwrap()[0].body["state"]["lines"]["L0"],
        "a\nb"
    );
}

#[test]
fn overflows_split_until_every_row_answers() {
    let folder = Folder::new();
    let mock = MockJev::start(|request| {
        if request.body["questions"].as_object().unwrap().len() > 2 {
            return (
                400,
                Vec::new(),
                r#"{"detail":{"error_type":"max_tokens_exceeded"}}"#.to_string(),
            );
        }

        ok_of(answered_of(&request.body, |_, line| {
            line.len() as f64 / 10.0
        }))
    });
    let outcome = outcome_of(
        &["Is this long?"],
        b"a\nbb\nccc\ndddd\neeeee\n",
        folder.environment_of(Some("test"), &mock),
    );

    assert_eq!(
        (text_of(&outcome.stdout), outcome.code),
        (
            "0.1\ta\n0.2\tbb\n0.3\tccc\n0.4\tdddd\n0.5\teeeee\n".to_string(),
            SUCCESS
        )
    );
    assert!(mock.requests.lock().unwrap().len() > 1);
}

#[test]
fn a_rate_limit_holds_the_next_request() {
    let folder = Folder::new();
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&arrivals);
    let mock = MockJev::start(move |request| {
        let mut arrivals = recorded.lock().unwrap();

        arrivals.push(Instant::now());

        if arrivals.len() == 1 {
            return (
                429,
                vec![("retry-after-ms".to_string(), "50".to_string())],
                r#"{"detail":"rate limited"}"#.to_string(),
            );
        }

        half(request)
    });
    let outcome = outcome_of(
        &["Is this a?"],
        b"a\nb\n",
        folder.environment_of(Some("test"), &mock),
    );

    assert_eq!(
        (text_of(&outcome.stdout), outcome.code),
        ("0.5\ta\n0.5\tb\n".to_string(), SUCCESS)
    );

    let arrivals = arrivals.lock().unwrap();

    assert_eq!(arrivals.len(), 2);
    assert!(arrivals[1] - arrivals[0] >= Duration::from_millis(50));
}

#[test]
fn an_unauthorized_key_exits_two_with_empty_stdout() {
    let folder = Folder::new();
    let mock = MockJev::start(|_| {
        (
            401,
            Vec::new(),
            r#"{"detail":"Invalid API key"}"#.to_string(),
        )
    });
    let outcome = outcome_of(
        &["Is this a?"],
        b"a\n",
        folder.environment_of(Some("test"), &mock),
    );

    assert_eq!(
        (outcome.stdout, outcome.stderr, outcome.code),
        (
            Vec::new(),
            "pyes: HTTP 401: Invalid API key\n".to_string(),
            FAILURE
        )
    );
}

#[test]
fn auth_stores_the_key_and_the_environment_key_wins_over_it() {
    let folder = Folder::new();
    let mock = MockJev::start(half);
    let stored = outcome_of(&["auth"], b"stored\n", folder.environment_of(None, &mock));

    assert_eq!((stored.code, stored.stderr), (SUCCESS, String::new()));
    assert_eq!(
        std::fs::read_to_string(folder.path.join("pyes").join("key")).unwrap(),
        "stored"
    );

    let from_file = outcome_of(&["Is this a?"], b"a\n", folder.environment_of(None, &mock));
    let from_environment = outcome_of(
        &["Is this a?"],
        b"a\n",
        folder.environment_of(Some("test"), &mock),
    );

    assert_eq!((from_file.code, from_environment.code), (SUCCESS, SUCCESS));

    let requests = mock.requests.lock().unwrap();
    let authorizations: Vec<Option<&str>> = requests
        .iter()
        .map(|request| request.header_of("authorization"))
        .collect();

    assert_eq!(
        authorizations,
        vec![Some("Bearer stored"), Some("Bearer test")]
    );
}
