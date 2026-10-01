#[allow(dead_code)]
#[path = "../src/mock_jev.rs"]
mod mock_jev;

mod integration {
    use super::mock_jev::MockJev;
    use serde_json::{json, Map};
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn scores_piped_lines_against_the_mock() {
        let mock = MockJev::start(|request| {
            let mut answers = Map::new();

            for id in request.body["questions"].as_object().unwrap().keys() {
                answers.insert(id.clone(), json!({ "type": "noul", "noul": 0.5 }));
            }

            (
                200,
                Vec::new(),
                json!({ "answers": answers, "usage": { "input_tokens": 1 } }).to_string(),
            )
        });
        let mut child = Command::new(env!("CARGO_BIN_EXE_pyes"))
            .args(["Is this a?", "Is this b?", "Is this c?"])
            .env("TYPESAFE_BASE_URL", &mock.url)
            .env("TYPESAFE_API_KEY", "test")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        child.stdin.take().unwrap().write_all(b"a\nb\nc\n").unwrap();

        let started_at = Instant::now();

        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }

            if started_at.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();

                panic!("pyes did not exit within 60 s");
            }

            std::thread::sleep(Duration::from_millis(10));
        };

        let mut stdout = String::new();
        let mut stderr = String::new();

        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();

        assert_eq!(
            (stdout.as_str(), stderr.as_str(), status.code()),
            (
                "0.5\t0.5\t0.5\ta\n0.5\t0.5\t0.5\tb\n0.5\t0.5\t0.5\tc\n",
                "",
                Some(0)
            )
        );
        assert_eq!(mock.requests.lock().unwrap().len(), 3);
    }
}
