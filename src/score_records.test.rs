use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

type Outcome = Result<(Vec<f64>, usize), Failure>;

#[derive(Clone)]
struct Call {
    question: String,
    records: Vec<String>,
    at: Instant,
}

fn call_of(body: &Value) -> Call {
    let lines = body["state"]["lines"].as_object().unwrap();
    let instructions = body["questions"]["L0"]["instructions"].as_str().unwrap();

    Call {
        question: instructions
            .trim_start_matches("For `lines.L0`, ")
            .to_string(),
        records: lines
            .values()
            .map(|line| line.as_str().unwrap().to_string())
            .collect(),
        at: Instant::now(),
    }
}

fn answered(call: &Call) -> Outcome {
    let offset = if call.question == "q1" { 100.0 } else { 0.0 };

    Ok((
        call.records
            .iter()
            .map(|record| offset + record[1..].parse::<f64>().unwrap())
            .collect(),
        0,
    ))
}

fn records_of(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("r{index}")).collect()
}

fn questions_of(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("Q{index}")).collect()
}

fn transient() -> Failure {
    Failure::Transient {
        message: "HTTP 503: busy".to_string(),
        retry_after: Some(Duration::from_millis(1)),
    }
}

fn rate_limited(milliseconds: u64) -> Failure {
    Failure::RateLimited {
        retry_after: Some(Duration::from_millis(milliseconds)),
    }
}

struct Run {
    result: Result<Vec<Vec<f64>>, String>,
    calls: Vec<Call>,
}

fn run_of(
    records: usize,
    questions: usize,
    ranges: Vec<Vec<Range<usize>>>,
    respond: impl Fn(&Call, usize) -> Outcome + std::marker::Send + Sync + 'static,
) -> Run {
    let (sender, receiver) = mpsc::channel();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&calls);

    std::thread::spawn(move || {
        let send = |body: &Value, _: usize| {
            let call = call_of(body);
            let mut calls = recorded.lock().unwrap_or_else(PoisonError::into_inner);
            let index = calls.len();

            calls.push(call.clone());
            drop(calls);

            respond(&call, index)
        };

        let _ = sender.send(score_ranges(
            &send,
            &records_of(records),
            &questions_of(questions),
            ranges,
        ));
    });

    let result = receiver
        .recv_timeout(Duration::from_secs(20))
        .expect("score_ranges did not return within 20 s");
    let calls = std::mem::take(&mut *calls.lock().unwrap_or_else(PoisonError::into_inner));

    Run { result, calls }
}

fn script_of(outcomes: Vec<Outcome>) -> impl Fn(&Call, usize) -> Outcome {
    let outcomes = Mutex::new(VecDeque::from(outcomes));

    move |call, _| {
        outcomes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(|| answered(call))
    }
}

#[test]
fn rows_land_in_input_order_across_questions() {
    let run = run_of(5, 2, vec![vec![0..2, 2..5], vec![0..3, 3..5]], |call, _| {
        answered(call)
    });

    assert_eq!(
        run.result,
        Ok(vec![
            vec![0.0, 1.0, 2.0, 3.0, 4.0],
            vec![100.0, 101.0, 102.0, 103.0, 104.0],
        ])
    );
    assert_eq!(run.calls.len(), 4);
}

#[test]
fn too_large_splits_until_single_records_answer() {
    let run = run_of(5, 1, vec![vec![0..5]], |call, _| {
        if call.records.len() > 1 {
            Err(Failure::TooLarge)
        } else {
            answered(call)
        }
    });

    assert_eq!(run.result, Ok(vec![vec![0.0, 1.0, 2.0, 3.0, 4.0]]));
    assert_eq!(run.calls.len(), 9);
}

#[test]
fn too_large_halves_at_the_midpoint() {
    let run = run_of(3, 1, vec![vec![0..3]], |call, index| {
        if index == 0 {
            Err(Failure::TooLarge)
        } else {
            answered(call)
        }
    });

    assert_eq!(run.result, Ok(vec![vec![0.0, 1.0, 2.0]]));

    let mut halves: Vec<Vec<String>> = run.calls[1..]
        .iter()
        .map(|call| call.records.clone())
        .collect();

    halves.sort();

    assert_eq!(
        halves,
        vec![
            vec!["r0".to_string()],
            vec!["r1".to_string(), "r2".to_string()]
        ]
    );
}

#[test]
fn a_single_record_too_large_names_its_record() {
    let run = run_of(3, 1, vec![vec![0..2, 2..3]], |call, _| {
        if call.records == vec!["r2".to_string()] {
            Err(Failure::TooLarge)
        } else {
            answered(call)
        }
    });

    assert_eq!(
        run.result,
        Err("record 3 exceeds Jev's request limits".to_string())
    );
}

#[test]
fn a_rate_limit_holds_dispatch_for_its_retry_after() {
    let run = run_of(
        1,
        1,
        vec![vec![0..1]],
        script_of(vec![Err(rate_limited(150))]),
    );

    assert_eq!(run.result, Ok(vec![vec![0.0]]));
    assert_eq!(run.calls.len(), 2);
    assert!(run.calls[1].at - run.calls[0].at >= Duration::from_millis(150));
}

#[test]
fn rate_limits_spend_no_retries() {
    let mut outcomes: Vec<Outcome> = (0..3).map(|_| Err(rate_limited(1))).collect();

    outcomes.extend((0..RETRIES).map(|_| Err(transient())));
    outcomes.extend((0..3).map(|_| Err(rate_limited(1))));

    let run = run_of(1, 1, vec![vec![0..1]], script_of(outcomes));

    assert_eq!(run.result, Ok(vec![vec![0.0]]));
    assert_eq!(run.calls.len(), 3 + RETRIES as usize + 3 + 1);
}

#[test]
fn a_zero_retry_after_backs_off_by_delay_of() {
    let run = run_of(
        1,
        1,
        vec![vec![0..1]],
        script_of(vec![Err(Failure::RateLimited {
            retry_after: Some(Duration::ZERO),
        })]),
    );

    assert_eq!(run.result, Ok(vec![vec![0.0]]));
    assert!(run.calls[1].at - run.calls[0].at >= Duration::from_millis(375));
}

#[test]
fn six_transients_error_after_five_retries() {
    let run = run_of(
        1,
        1,
        vec![vec![0..1]],
        script_of((0..=RETRIES).map(|_| Err(transient())).collect()),
    );

    assert_eq!(
        run.result,
        Err("HTTP 503: busy after 5 retries".to_string())
    );
    assert_eq!(run.calls.len(), 6);
}

#[test]
fn five_transients_then_success_answers() {
    let run = run_of(
        1,
        1,
        vec![vec![0..1]],
        script_of((0..RETRIES).map(|_| Err(transient())).collect()),
    );

    assert_eq!(run.result, Ok(vec![vec![0.0]]));
    assert_eq!(run.calls.len(), 6);
}

#[test]
fn a_rate_limit_between_transients_keeps_the_retry_count() {
    let mut outcomes = vec![Err(transient()), Err(rate_limited(1))];

    outcomes.extend((0..RETRIES).map(|_| Err(transient())));

    let run = run_of(1, 1, vec![vec![0..1]], script_of(outcomes));

    assert_eq!(
        run.result,
        Err("HTTP 503: busy after 5 retries".to_string())
    );
}

#[test]
fn a_fatal_stops_dispatch_while_in_flight_jobs_finish() {
    let finished = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&finished);
    let run = run_of(3, 1, vec![vec![0..1, 1..3]], move |call, _| {
        if call.records == vec!["r0".to_string()] {
            return Err(Failure::Fatal("HTTP 401: denied".to_string()));
        }

        std::thread::sleep(Duration::from_millis(100));
        counted.fetch_add(1, Ordering::SeqCst);

        Err(Failure::TooLarge)
    });

    assert_eq!(run.result, Err("HTTP 401: denied".to_string()));
    assert_eq!(finished.load(Ordering::SeqCst), 1);
    assert_eq!(run.calls.len(), 2);
}

#[test]
fn a_panicking_send_returns_the_internal_error() {
    let run = run_of(2, 1, vec![vec![0..1, 1..2]], |call, _| {
        if call.records == vec!["r1".to_string()] {
            panic!("send failed");
        }

        answered(call)
    });

    assert_eq!(
        run.result,
        Err("internal error: a request thread panicked".to_string())
    );
}

#[test]
fn a_given_retry_after_is_the_delay() {
    assert_eq!(
        delay_of(3, Some(Duration::from_millis(1234))),
        Duration::from_millis(1234)
    );
}

#[test]
fn backoff_doubles_from_500_ms_to_a_5_s_cap() {
    let delays: Vec<Duration> = [0, 1, 2, 3, 4, 1_000]
        .into_iter()
        .map(|attempt| backoff_of(attempt, 0.0))
        .collect();

    assert_eq!(
        delays,
        [500, 1_000, 2_000, 4_000, 5_000, 5_000].map(Duration::from_millis)
    );
}

#[test]
fn full_jitter_takes_a_quarter_off() {
    assert_eq!(backoff_of(0, 1.0), Duration::from_millis(375));
    assert_eq!(backoff_of(4, 1.0), Duration::from_millis(3_750));
}

#[test]
fn jitter_stays_within_75_to_100_percent() {
    for attempt in [0, 1, 4, 1_000] {
        let base = backoff_of(attempt, 0.0);

        for _ in 0..200 {
            let delay = delay_of(attempt, None);

            assert!(delay <= base && delay >= base.mul_f64(0.75));
        }
    }
}
