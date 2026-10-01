use super::*;
use crate::estimate_of::tokens_of;

fn bytes_of(text: &str) -> usize {
    text.len()
}

fn records_of(count: usize, text: &str) -> Vec<String> {
    vec![text.to_string(); count]
}

// One token per byte, spelled from the Approach's sums without chunks_of.
fn raw_of(records: &[String], question: &str, count: usize) -> (usize, usize, usize) {
    let mut raw = "{\"lines\":{}}".len();
    let mut instruction_tokens = 0;
    let mut longest = 0;

    for (index, record) in records.iter().take(count).enumerate() {
        let instruction = instructions_of(question, index).len();

        raw += format!("\"L{index}\":\"{record}\",").len() + instruction;
        instruction_tokens += instruction;
        longest = longest.max(instruction);
    }

    (raw, instruction_tokens, longest)
}

fn request_fits(records: &[String], question: &str, count: usize) -> bool {
    let (raw, _, _) = raw_of(records, question, count);

    count <= MAX_QUESTIONS && estimate_of(raw, count) as f64 <= HEADROOM * REQUEST_LIMIT as f64
}

fn state_fits(records: &[String], question: &str, count: usize) -> bool {
    let (raw, instruction_tokens, longest) = raw_of(records, question, count);

    estimate_of(raw - instruction_tokens + longest, 1) as f64 <= HEADROOM * STATE_LIMIT as f64
}

fn largest_fit(records: &[String], fits: &dyn Fn(usize) -> bool) -> usize {
    (1..=records.len())
        .take_while(|count| fits(*count))
        .last()
        .unwrap()
}

#[test]
fn everything_in_one_chunk() {
    let records = records_of(10, "short");

    assert_eq!(
        chunks_of(&records, "Is it?", &bytes_of),
        Ok(Vec::from_iter(std::iter::once(0..10)))
    );
}

#[test]
fn no_records_make_no_chunks() {
    assert_eq!(chunks_of(&[], "Is it?", &bytes_of), Ok(vec![]));
}

#[test]
fn splits_exactly_at_the_limit() {
    let records = records_of(1_000, &"a".repeat(100));
    let fits = largest_fit(&records, &|count| {
        request_fits(&records, "Is it?", count) && state_fits(&records, "Is it?", count)
    });

    assert!(fits > 1 && fits < 1_000);

    let chunks = chunks_of(&records, "Is it?", &bytes_of).unwrap();

    assert_eq!(chunks[0], 0..fits);
    assert_eq!(chunks[1].start, fits);
    assert_eq!(chunks.last().unwrap().end, 1_000);
}

#[test]
fn the_state_limit_binds_first_for_long_records() {
    let records = records_of(1_000, &"a".repeat(100));
    let by_state = largest_fit(&records, &|count| state_fits(&records, "Is it?", count));
    let by_request = largest_fit(&records, &|count| request_fits(&records, "Is it?", count));

    assert!(by_state < by_request);
    assert_eq!(
        chunks_of(&records, "Is it?", &bytes_of).unwrap()[0],
        0..by_state
    );
}

#[test]
fn the_request_limit_binds_first_for_many_short_records() {
    let question = "Is this record about a very specific topic?";
    let records = records_of(1_500, "a");
    let by_state = largest_fit(&records, &|count| state_fits(&records, question, count));
    let by_request = largest_fit(&records, &|count| request_fits(&records, question, count));

    assert!(by_request < by_state && by_request < MAX_QUESTIONS);
    assert_eq!(
        chunks_of(&records, question, &bytes_of).unwrap()[0],
        0..by_request
    );
}

#[test]
fn caps_a_chunk_at_the_question_maximum() {
    let records = records_of(2_000, "a");

    assert_eq!(
        chunks_of(&records, "?", &bytes_of),
        Ok(vec![0..1_024, 1_024..2_000])
    );
}

#[test]
fn a_single_oversize_record_names_its_number() {
    let mut records = records_of(3, "short");

    records[1] = "a".repeat(40_000);

    assert_eq!(
        chunks_of(&records, "Is it?", &bytes_of),
        Err("record 2 exceeds Jev's request limits".to_string())
    );

    records[0] = "a".repeat(40_000);

    assert_eq!(
        chunks_of(&records, "Is it?", &bytes_of),
        Err("record 1 exceeds Jev's request limits".to_string())
    );
}

#[test]
fn counts_with_the_real_tokenizer() {
    let records = records_of(1_100, "a short line");

    assert_eq!(
        chunks_of(&records, "Is this a line?", &tokens_of),
        Ok(vec![0..1_024, 1_024..1_100])
    );
}
