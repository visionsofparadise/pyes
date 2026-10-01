pub const STATE_LIMIT: usize = 32_000;
pub const REQUEST_LIMIT: usize = 64_000;
pub const HEADROOM: f64 = 0.9;
pub const MAX_QUESTIONS: usize = 1_024;

pub fn tokens_of(text: &str) -> usize {
    tiktoken_rs::o200k_base_singleton().count_ordinary(text)
}

pub fn estimate_of(raw: usize, questions: usize) -> usize {
    (1.08 * raw as f64 + 15.0 * questions as f64 + 227.0).ceil() as usize
}
