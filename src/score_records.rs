use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::ops::Range;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
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

pub type Send<'a> = dyn Fn(&Value, usize) -> Result<(Vec<f64>, usize), Failure> + Sync + 'a;

fn backoff_of(attempt: u32, jitter: f64) -> Duration {
    let exponential = (BACKOFF_INITIAL * 2u32.pow(attempt.min(4))).min(BACKOFF_LIMIT);

    exponential.mul_f64(1.0 - jitter * JITTER)
}

fn jitter_of() -> f64 {
    let bits = RandomState::new().build_hasher().finish() >> 11;

    bits as f64 / (1u64 << 53) as f64
}

pub fn delay_of(attempt: u32, retry_after: Option<Duration>) -> Duration {
    retry_after.unwrap_or_else(|| backoff_of(attempt, jitter_of()))
}

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
    held: Duration,
    fatal: Option<String>,
}

impl State {
    fn record(&mut self, message: String) {
        self.fatal.get_or_insert(message);
    }

    fn answer(&mut self, job: &Job, answers: &[f64]) {
        self.columns[job.question][job.range.clone()].copy_from_slice(answers);

        self.consecutive_rate_limits = 0;
    }

    fn hold(&mut self, job: Job, retry_after: Option<Duration>) {
        self.consecutive_rate_limits += 1;

        let now = Instant::now();
        let held_from = self.resume_at.max(now);
        let until = now
            + delay_of(
                self.consecutive_rate_limits - 1,
                retry_after.filter(|delay| !delay.is_zero()),
            );

        if until > held_from {
            self.held += until - held_from;
            self.resume_at = until;
        }

        if self.held > HOLD_LIMIT {
            self.record("rate limited for 10 minutes".to_string());
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
                state.hold(job, retry_after);

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

                let due = Instant::now() + delay_of(job.retries, retry_after);

                loop {
                    if state.fatal.is_some() {
                        return;
                    }

                    let until = due.max(state.resume_at);
                    let now = Instant::now();

                    if until <= now {
                        break;
                    }

                    state = shared
                        .changed
                        .wait_timeout(state, until - now)
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

                let now = Instant::now();

                if state.resume_at > now {
                    let timeout = state.resume_at - now;

                    state = shared
                        .changed
                        .wait_timeout(state, timeout)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;

                    continue;
                }

                if let Some(job) = state.queue.pop_front() {
                    let shared = &shared;
                    let spawned = std::thread::Builder::new().spawn_scoped(scope, move || {
                        run_job(shared, send, records, questions, job)
                    });

                    match spawned {
                        Ok(_) => state.in_flight += 1,
                        Err(error) => {
                            state.record(format!("could not start a request thread: {error}"));

                            shared.changed.notify_all();
                        }
                    }
                }
            }
        });
    }));

    if dispatched.is_err() {
        return Err("internal error: a request thread panicked".to_string());
    }

    let state = shared
        .state
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);

    match state.fatal {
        Some(message) => Err(message),
        None => Ok(state.columns),
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
