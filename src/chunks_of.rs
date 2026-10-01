use std::ops::Range;

use crate::estimate_of::{estimate_of, HEADROOM, MAX_QUESTIONS, REQUEST_LIMIT, STATE_LIMIT};
use crate::request_of::instructions_of;

struct Chunk {
    start: usize,
    count: usize,
    raw: usize,
    instruction_tokens: usize,
    longest_instruction: usize,
}

impl Chunk {
    fn new(start: usize, base: usize) -> Chunk {
        Chunk {
            start,
            count: 0,
            raw: base,
            instruction_tokens: 0,
            longest_instruction: 0,
        }
    }

    fn admits(&self, entry: usize, instruction: usize) -> bool {
        let count = self.count + 1;
        let raw = self.raw + entry + instruction;
        let state_raw = raw - (self.instruction_tokens + instruction)
            + self.longest_instruction.max(instruction);

        count <= MAX_QUESTIONS
            && estimate_of(raw, count) as f64 <= HEADROOM * REQUEST_LIMIT as f64
            && estimate_of(state_raw, 1) as f64 <= HEADROOM * STATE_LIMIT as f64
    }

    fn push(&mut self, entry: usize, instruction: usize) {
        self.count += 1;
        self.raw += entry + instruction;
        self.instruction_tokens += instruction;
        self.longest_instruction = self.longest_instruction.max(instruction);
    }
}

pub fn chunks_of(
    records: &[String],
    question: &str,
    tokens_of: &dyn Fn(&str) -> usize,
) -> Result<Vec<Range<usize>>, String> {
    let base = tokens_of("{\"lines\":{}}");
    let costs_of = |record: &str, index: usize| {
        let entry = format!("\"L{index}\":{},", serde_json::Value::from(record));

        (
            tokens_of(&entry),
            tokens_of(&instructions_of(question, index)),
        )
    };
    let mut chunks = Vec::new();
    let mut chunk = Chunk::new(0, base);

    for (number, record) in records.iter().enumerate() {
        let (mut entry, mut instruction) = costs_of(record, chunk.count);

        if !chunk.admits(entry, instruction) {
            if chunk.count > 0 {
                chunks.push(chunk.start..number);

                chunk = Chunk::new(number, base);
                (entry, instruction) = costs_of(record, 0);
            }

            if !chunk.admits(entry, instruction) {
                return Err(format!(
                    "record {} exceeds Jev's request limits",
                    number + 1
                ));
            }
        }

        chunk.push(entry, instruction);
    }

    if chunk.count > 0 {
        chunks.push(chunk.start..records.len());
    }

    Ok(chunks)
}

#[cfg(test)]
#[path = "chunks_of.test.rs"]
mod tests;
