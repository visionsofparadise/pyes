use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io;
use std::ops::Range;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::Scope;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::answers_of::Failure;
use crate::attempt::{attempt, Client};
use crate::chunks_of::chunks_of;
use crate::estimate_of::tokens_of;
use crate::request_of::request_of;

pub const RETRIES: u32 = 5;
pub const HOLD_LIMIT: Duration = Duration::from_secs(600);

const BACKOFF_INITIAL: Duration = Duration::from_millis(500);
const BACKOFF_LIMIT: Duration = Duration::from_secs(5);
const JITTER: f64 = 0.25;
const RATE_LIMITED: &str = "rate limited";

pub type Send<'a> = dyn Fn(&Value, usize) -> Result<(Vec<f64>, usize), Failure> + Sync + 'a;

type Task<'scope> = Box<dyn FnOnce() + std::marker::Send + 'scope>;

trait Spawner {
    fn spawn<'scope, 'env>(
        &mut self,
        scope: &'scope Scope<'scope, 'env>,
        task: Task<'scope>,
    ) -> io::Result<()>;
}

struct Threads;

impl Spawner for Threads {
    fn spawn<'scope, 'env>(
        &mut self,
        scope: &'scope Scope<'scope, 'env>,
        task: Task<'scope>,
    ) -> io::Result<()> {
        std::thread::Builder::new()
            .spawn_scoped(scope, task)
            .map(|_| ())
    }
}

fn backoff_of(attempt: u32, jitter: f64) -> Duration {
    let exponential = (BACKOFF_INITIAL * 2u32.pow(attempt.min(4))).min(BACKOFF_LIMIT);

    exponential.mul_f64(1.0 - jitter * JITTER)
}

fn jitter_of() -> f64 {
    let bits = RandomState::new().build_hasher().finish() >> 11;

    bits as f64 / (1u64 << 53) as f64
}

pub fn delay_of(attempt: u32, retry_after: Option<Duration>) -> Duration {
    retry_after
        .filter(|delay| !delay.is_zero())
        .unwrap_or_else(|| backoff_of(attempt, jitter_of()))
}

#[derive(Clone)]
struct Job {
    question: usize,
    range: Range<usize>,
    retries: u32,
}

struct State {
    queue: VecDeque<Job>,
    columns: Vec<Vec<f64>>,
    in_flight: usize,
    resume_at: Instant,
    consecutive_rate_limits: u32,
    rate_limited_at: Option<Instant>,
    held: Duration,
    fatal: Option<String>,
}

impl State {
    fn record(&mut self, message: String) {
        self.fatal.get_or_insert(message);
    }

    fn rate_limited_of(&self) -> Duration {
        self.held
            + self
                .rate_limited_at
                .map_or(Duration::ZERO, |rate_limited_at| {
                    self.resume_at.saturating_duration_since(rate_limited_at)
                })
    }

    fn answer(&mut self, job: &Job, answers: &[f64]) {
        self.columns[job.question][job.range.clone()].copy_from_slice(answers);

        self.held = self.rate_limited_of();
        self.rate_limited_at = None;
        self.consecutive_rate_limits = 0;
    }

    fn hold(&mut self, job: Job, retry_after: Option<Duration>, cause: &str) {
        self.hold_from(job, retry_after, cause, Instant::now());
    }

    fn hold_from(
        &mut self,
        job: Job,
        retry_after: Option<Duration>,
        cause: &str,
        refused_at: Instant,
    ) {
        self.consecutive_rate_limits += 1;

        self.rate_limited_at
            .get_or_insert(self.resume_at.max(refused_at));

        self.resume_at = self
            .resume_at
            .max(refused_at + delay_of(self.consecutive_rate_limits - 1, retry_after));

        if self.rate_limited_of() > HOLD_LIMIT {
            self.record(format!("{cause} for 10 minutes"));
        }

        self.queue.push_front(job);
    }

    fn split(&mut self, job: Job) {
        if job.range.len() == 1 {
            self.record(format!(
                "record {} exceeds Jev's request limits",
                job.range.start + 1
            ));

            return;
        }

        let middle = job.range.start + job.range.len() / 2;

        self.queue.push_front(Job {
            question: job.question,
            range: middle..job.range.end,
            retries: job.retries,
        });
        self.queue.push_front(Job {
            question: job.question,
            range: job.range.start..middle,
            retries: job.retries,
        });
    }
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct InFlight<'a> {
    shared: &'a Shared,
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        let mut state = self.shared.lock();

        state.in_flight -= 1;

        if std::thread::panicking() {
            state.record("internal error: a request thread panicked".to_string());
        }

        self.shared.changed.notify_all();
    }
}

fn due_at_of(job: &Job, retry_after: Option<Duration>, failed_at: Instant) -> Instant {
    failed_at + delay_of(job.retries, retry_after)
}

fn run_job(shared: &Shared, send: &Send, records: &[String], questions: &[String], mut job: Job) {
    let _in_flight = InFlight { shared };
    let body = request_of(&records[job.range.clone()], &questions[job.question]);

    loop {
        let outcome = send(&body, job.range.len());
        let mut state = shared.lock();

        match outcome {
            Ok((answers, _)) => {
                state.answer(&job, &answers);

                return;
            }
            Err(Failure::RateLimited { retry_after }) => {
                state.hold(job, retry_after, RATE_LIMITED);

                return;
            }
            Err(Failure::Exhausted(message)) => {
                if state.in_flight > 1 {
                    state.hold(job, None, &message);
                } else {
                    state.record(message);
                }

                return;
            }
            Err(Failure::Transient {
                message,
                retry_after,
            }) => {
                if job.retries >= RETRIES {
                    state.record(format!("{message} after {RETRIES} retries"));

                    return;
                }

                let due_at = due_at_of(&job, retry_after, Instant::now());

                loop {
                    if state.fatal.is_some() {
                        return;
                    }

                    let send_at = due_at.max(state.resume_at);
                    let checked_at = Instant::now();

                    if send_at <= checked_at {
                        break;
                    }

                    state = shared
                        .changed
                        .wait_timeout(state, send_at - checked_at)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }

                job.retries += 1;
            }
            Err(Failure::TooLarge) => {
                state.split(job);

                return;
            }
            Err(Failure::Fatal(message)) => {
                state.record(message);

                return;
            }
        }
    }
}

pub fn score_ranges(
    send: &Send,
    records: &[String],
    questions: &[String],
    ranges: Vec<Vec<Range<usize>>>,
) -> Result<Vec<Vec<f64>>, String> {
    score_ranges_over(&mut Threads, send, records, questions, ranges)
}

fn score_ranges_over(
    spawner: &mut dyn Spawner,
    send: &Send,
    records: &[String],
    questions: &[String],
    ranges: Vec<Vec<Range<usize>>>,
) -> Result<Vec<Vec<f64>>, String> {
    let queue = ranges
        .into_iter()
        .enumerate()
        .flat_map(|(question, ranges)| {
            ranges.into_iter().map(move |range| Job {
                question,
                range,
                retries: 0,
            })
        })
        .collect();
    let shared = Shared {
        state: Mutex::new(State {
            queue,
            columns: vec![vec![0.0; records.len()]; questions.len()],
            in_flight: 0,
            resume_at: Instant::now(),
            consecutive_rate_limits: 0,
            rate_limited_at: None,
            held: Duration::ZERO,
            fatal: None,
        }),
        changed: Condvar::new(),
    };

    let dispatched = catch_unwind(AssertUnwindSafe(|| {
        std::thread::scope(|scope| {
            let mut state = shared.lock();

            loop {
                let idle = state.fatal.is_some() || state.queue.is_empty();

                if idle && state.in_flight == 0 {
                    return;
                }

                if idle {
                    state = shared
                        .changed
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner);

                    continue;
                }

                let checked_at = Instant::now();

                if state.resume_at > checked_at {
                    let timeout = state.resume_at - checked_at;

                    state = shared
                        .changed
                        .wait_timeout(state, timeout)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;

                    continue;
                }

                if let Some(job) = state.queue.pop_front() {
                    state.in_flight += 1;

                    drop(state);

                    let shared = &shared;
                    let task_job = job.clone();
                    let spawned = spawner.spawn(
                        scope,
                        Box::new(move || run_job(shared, send, records, questions, task_job)),
                    );

                    state = shared.lock();

                    if let Err(error) = spawned {
                        let message = format!("could not start a request thread: {error}");

                        state.in_flight -= 1;

                        if state.in_flight == 0 {
                            state.record(message);
                        } else {
                            state.hold(job, None, &message);
                        }

                        shared.changed.notify_all();
                    }
                }
            }
        });
    }));

    let State { fatal, columns, .. } = shared
        .state
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);

    match (fatal, dispatched) {
        (Some(message), _) => Err(message),
        (None, Err(_)) => Err("internal error: pyes panicked".to_string()),
        (None, Ok(())) => Ok(columns),
    }
}

pub fn score_records(
    client: &Client,
    records: &[String],
    questions: &[String],
) -> Result<Vec<Vec<f64>>, String> {
    let ranges = questions
        .iter()
        .map(|question| chunks_of(records, question, &tokens_of))
        .collect::<Result<Vec<_>, String>>()?;

    score_ranges(
        &|body, count| attempt(client, body, count),
        records,
        questions,
        ranges,
    )
}

#[cfg(test)]
#[path = "score_records.test.rs"]
mod tests;
