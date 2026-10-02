use super::*;
use std::collections::HashSet;
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

struct Spawned {
    threads: usize,
}

impl Spawner for Spawned {
    fn spawn<'scope, 'env>(
        &mut self,
        scope: &'scope Scope<'scope, 'env>,
        task: Task<'scope>,
    ) -> io::Result<()> {
        if self.threads == 0 {
            task();

            return Ok(());
        }

        self.threads -= 1;

        Threads.spawn(scope, task)
    }
}

struct Refusing {
    threads: usize,
    refusals: usize,
    refused_at: Arc<Mutex<Option<Instant>>>,
}

impl Spawner for Refusing {
    fn spawn<'scope, 'env>(
        &mut self,
        scope: &'scope Scope<'scope, 'env>,
        task: Task<'scope>,
    ) -> io::Result<()> {
        if self.threads > 0 {
            self.threads -= 1;

            return Threads.spawn(scope, task);
        }

        if self.refusals > 0 {
            self.refusals -= 1;
            *self
                .refused_at
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());

            return Err(io::Error::other("no threads left"));
        }

        task();

        Ok(())
    }
}

#[derive(Clone, Default)]
struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    fn open(&self) {
        let (opened, changed) = &*self.0;

        *opened.lock().unwrap_or_else(PoisonError::into_inner) = true;

        changed.notify_all();
    }

    fn wait(&self) {
        let (opened, changed) = &*self.0;
        let opened = opened.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = changed
            .wait_timeout_while(opened, Duration::from_secs(10), |opened| !*opened)
            .unwrap_or_else(PoisonError::into_inner);
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
    run_over(Threads, records, questions, ranges, respond)
}

fn run_over(
    mut spawner: impl Spawner + std::marker::Send + 'static,
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

        let _ = sender.send(score_ranges_over(
            &mut spawner,
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
fn a_rate_limit_holds_the_next_pop_for_its_retry_after() {
    let run = run_over(
        Spawned { threads: 0 },
        3,
        1,
        vec![vec![0..1, 1..2, 2..3]],
        script_of(vec![Err(rate_limited(150))]),
    );
    let sent: Vec<&str> = run
        .calls
        .iter()
        .map(|call| call.records[0].as_str())
        .collect();

    assert_eq!(run.result, Ok(vec![vec![0.0, 1.0, 2.0]]));
    assert_eq!(sent, ["r0", "r0", "r1", "r2"]);
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
fn a_transient_retry_waits_for_its_retry_after() {
    let run = run_of(
        1,
        1,
        vec![vec![0..1]],
        script_of(vec![Err(Failure::Transient {
            message: "HTTP 503: busy".to_string(),
            retry_after: Some(Duration::from_millis(1200)),
        })]),
    );

    assert_eq!(run.result, Ok(vec![vec![0.0]]));
    assert!(run.calls[1].at - run.calls[0].at >= Duration::from_millis(1200));
}

#[test]
fn transient_retries_without_a_retry_after_climb_the_backoff() {
    let bare = || Failure::Transient {
        message: "HTTP 503: busy".to_string(),
        retry_after: None,
    };
    let run = run_of(
        1,
        1,
        vec![vec![0..1]],
        script_of(vec![Err(bare()), Err(bare())]),
    );

    assert_eq!(run.result, Ok(vec![vec![0.0]]));
    assert!(run.calls[2].at - run.calls[1].at >= Duration::from_millis(750));
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
    let gate = Gate::default();
    let run = run_over(
        Spawned { threads: 1 },
        4,
        1,
        vec![vec![0..2, 2..3, 3..4]],
        move |call, _| {
            if call.records[0] == "r2" {
                gate.open();

                return Err(Failure::Fatal("HTTP 401: denied".to_string()));
            }

            gate.wait();
            counted.fetch_add(1, Ordering::SeqCst);

            Err(Failure::TooLarge)
        },
    );

    assert_eq!(run.result, Err("HTTP 401: denied".to_string()));
    assert_eq!(finished.load(Ordering::SeqCst), 1);
    assert_eq!(run.calls.len(), 2);
}

#[test]
fn a_fatal_stops_dispatch_before_the_next_pop() {
    const JOBS: usize = 100;

    let run = run_over(
        Spawned { threads: 0 },
        JOBS,
        1,
        vec![(0..JOBS).map(|index| index..index + 1).collect()],
        |call, _| {
            if call.records[0] == "r0" {
                return Err(Failure::Fatal("HTTP 401: denied".to_string()));
            }

            answered(call)
        },
    );

    assert_eq!(run.result, Err("HTTP 401: denied".to_string()));
    assert_eq!(run.calls.len(), 1);
}

#[test]
fn a_failed_spawn_with_nothing_in_flight_is_fatal() {
    let spawner = Refusing {
        threads: 0,
        refusals: 1,
        refused_at: Arc::default(),
    };
    let run = run_over(spawner, 2, 1, vec![vec![0..1, 1..2]], |call, _| {
        answered(call)
    });

    assert_eq!(
        run.result,
        Err("could not start a request thread: no threads left".to_string())
    );
    assert_eq!(run.calls.len(), 0);
}

#[test]
fn a_failed_spawn_while_jobs_are_in_flight_requeues_and_holds() {
    let refused_at = Arc::default();
    let spawner = Refusing {
        threads: 1,
        refusals: 1,
        refused_at: Arc::clone(&refused_at),
    };
    let retried = Gate::default();
    let run = run_over(spawner, 2, 1, vec![vec![0..1, 1..2]], move |call, _| {
        if call.records[0] == "r0" {
            retried.wait();
        } else {
            retried.open();
        }

        answered(call)
    });
    let refused_at = refused_at
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .expect("the second spawn was not refused");
    let retry = run
        .calls
        .iter()
        .find(|call| call.records[0] == "r1")
        .expect("the refused job was not sent");

    assert_eq!(run.result, Ok(vec![vec![0.0, 1.0]]));
    assert_eq!(run.calls.len(), 2);
    assert!(retry.at - refused_at >= Duration::from_millis(375));
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

#[test]
fn jitter_varies_the_delay() {
    let delays: std::collections::HashSet<Duration> = (0..200).map(|_| delay_of(0, None)).collect();

    assert!(delays.len() > 1);
}

fn first_sends_of(
    respond: impl Fn(&Call) -> Outcome + std::marker::Send + Sync + 'static,
) -> impl Fn(&Call, usize) -> Outcome + std::marker::Send + Sync + 'static {
    let sent = Mutex::new(HashSet::new());

    move |call, _| {
        let first = sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(call.records[0].clone());

        if first {
            respond(call)
        } else {
            answered(call)
        }
    }
}

#[test]
fn a_transient_retry_waits_out_a_hold_set_during_its_wait() {
    let gate = Gate::default();
    let run = run_over(
        Spawned { threads: 1 },
        2,
        1,
        vec![vec![0..1, 1..2]],
        first_sends_of(move |call| {
            if call.records[0] == "r0" {
                gate.open();

                return Err(Failure::Transient {
                    message: "HTTP 503: busy".to_string(),
                    retry_after: Some(Duration::from_millis(300)),
                });
            }

            gate.wait();

            Err(rate_limited(600))
        }),
    );

    assert_eq!(run.result, Ok(vec![vec![0.0, 1.0]]));

    let sends_of = |record: &str| -> Vec<Instant> {
        run.calls
            .iter()
            .filter(|call| call.records[0] == record)
            .map(|call| call.at)
            .collect()
    };

    assert!(sends_of("r0")[1] - sends_of("r1")[0] >= Duration::from_millis(600));
}

#[test]
fn a_fatal_ends_a_transient_wait_without_another_send() {
    let gate = Gate::default();
    let run = run_over(
        Spawned { threads: 1 },
        2,
        1,
        vec![vec![0..1, 1..2]],
        move |call, _| {
            if call.records[0] == "r0" {
                gate.open();

                return Err(Failure::Transient {
                    message: "HTTP 503: busy".to_string(),
                    retry_after: Some(Duration::from_secs(30)),
                });
            }

            gate.wait();

            Err(Failure::Fatal("HTTP 401: denied".to_string()))
        },
    );

    assert_eq!(run.result, Err("HTTP 401: denied".to_string()));
    assert_eq!(run.calls.len(), 2);
}

fn job_of(range: Range<usize>, retries: u32) -> Job {
    Job {
        question: 0,
        range,
        retries,
    }
}

fn state_of() -> State {
    State {
        queue: VecDeque::new(),
        columns: vec![vec![0.0]],
        in_flight: 0,
        resume_at: Instant::now(),
        consecutive_rate_limits: 0,
        held: Duration::ZERO,
        fatal: None,
    }
}

fn queued_of(state: &State) -> Vec<(Range<usize>, u32)> {
    state
        .queue
        .iter()
        .map(|job| (job.range.clone(), job.retries))
        .collect()
}

#[test]
fn the_first_bare_rate_limit_backs_off_from_500_ms() {
    let mut state = state_of();

    state.hold(job_of(0..1, 0), None);

    assert!(state.held >= Duration::from_millis(375) && state.held <= Duration::from_millis(500));
}

#[test]
fn repeated_bare_rate_limits_climb_the_backoff() {
    let mut state = state_of();

    for _ in 0..3 {
        state.hold(job_of(0..1, 0), None);
    }

    assert!(state.held > Duration::from_millis(1_000));
}

#[test]
fn a_success_restarts_the_rate_limit_backoff() {
    let mut state = state_of();

    for _ in 0..3 {
        state.hold(job_of(0..1, 0), None);
    }

    state.answer(&job_of(0..1, 0), &[0.5]);

    state.resume_at = Instant::now();
    state.held = Duration::ZERO;

    state.hold(job_of(0..1, 0), None);

    assert_eq!(state.columns, vec![vec![0.5]]);
    assert!(state.held <= Duration::from_millis(500));
}

#[test]
fn overlapping_holds_charge_only_their_extension() {
    let mut state = state_of();

    state.hold(job_of(0..1, 0), Some(Duration::from_secs(400)));
    state.hold(job_of(0..1, 0), Some(Duration::from_secs(400)));

    assert!(state.held < Duration::from_secs(401));
    assert_eq!(state.fatal, None);
}

#[test]
fn a_shorter_rate_limit_leaves_a_longer_hold_in_place() {
    let mut state = state_of();

    state.hold(job_of(0..1, 0), Some(Duration::from_secs(10)));
    state.hold(job_of(0..1, 0), Some(Duration::from_millis(1)));

    assert!(state.resume_at > Instant::now() + Duration::from_secs(5));
}

#[test]
fn a_hold_past_the_limit_is_fatal() {
    let mut state = state_of();

    state.hold(job_of(0..1, 0), Some(HOLD_LIMIT + Duration::from_secs(1)));

    assert_eq!(state.fatal, Some("rate limited for 10 minutes".to_string()));
}

#[test]
fn the_first_fatal_is_the_one_reported() {
    let mut state = state_of();

    state.record("HTTP 401: denied".to_string());
    state.record("internal error: a request thread panicked".to_string());

    assert_eq!(state.fatal, Some("HTTP 401: denied".to_string()));
}

#[test]
fn a_rate_limited_job_goes_back_ahead_of_the_queue() {
    let mut state = state_of();

    state.queue.push_back(job_of(1..2, 0));
    state.hold(job_of(0..1, 2), Some(Duration::from_millis(1)));

    assert_eq!(queued_of(&state), vec![(0..1, 2), (1..2, 0)]);
}

#[test]
fn split_halves_go_ahead_of_the_queue_first_half_first_with_the_retries_spent() {
    let mut state = state_of();

    state.queue.push_back(job_of(5..6, 0));
    state.split(job_of(0..5, 3));

    assert_eq!(queued_of(&state), vec![(0..2, 3), (2..5, 3), (5..6, 0)]);
}

#[test]
fn a_panic_stops_further_dispatch() {
    let gate = Gate::default();
    let run = run_of(
        2,
        1,
        vec![vec![0..1, 1..2]],
        first_sends_of(move |call| {
            if call.records[0] == "r1" {
                gate.open();

                panic!("send failed");
            }

            gate.wait();

            Err(rate_limited(10_000))
        }),
    );

    assert_eq!(
        run.result,
        Err("internal error: a request thread panicked".to_string())
    );
    assert_eq!(run.calls.len(), 2);
}
