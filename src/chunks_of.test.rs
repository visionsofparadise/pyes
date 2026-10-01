use super::*;
use crate::estimate_of::tokens_of;

fn bytes_of(text: &str) -> usize {
    text.len()
}

fn weighted_bytes_of(text: &str) -> usize {
    text.len() + if text.contains("lines.L0`") { 50 } else { 0 }
}

fn records_of(count: usize, text: &str) -> Vec<String> {
    vec![text.to_string(); count]
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
fn the_request_limit_admits_the_record_that_lands_on_it() {
    let question = format!("Is {}", "x".repeat(3_997));
    let records: Vec<String> = (0..27)
        .map(|index| "a".repeat(if index % 12 == 11 { 395 } else { 387 }))
        .collect();

    assert_eq!(
        chunks_of(&records, &question, &bytes_of),
        Ok(vec![0..12, 12..24, 24..27])
    );
}

#[test]
fn the_state_limit_admits_the_record_that_lands_on_it() {
    let records = records_of(25, &"a".repeat(2_393));

    assert_eq!(
        chunks_of(&records, "?", &bytes_of),
        Ok(vec![0..11, 11..22, 22..25])
    );
}

#[test]
fn the_request_limit_fits_a_record_that_lands_one_token_inside_it() {
    let question = "?".repeat(3_990);
    let mut records = records_of(11, &"a".repeat(380));

    records.push("a".repeat(592));

    assert_eq!(
        chunks_of(&records, &question, &bytes_of),
        Ok(Vec::from_iter(std::iter::once(0..12)))
    );

    records[11].push('a');

    assert_eq!(
        chunks_of(&records, &question, &bytes_of),
        Ok(vec![0..11, 11..12])
    );
}

#[test]
fn the_state_limit_fits_a_record_that_lands_one_token_inside_it() {
    let mut records = records_of(12, &"a".repeat(2_000));

    records.push("a".repeat(2_256));

    assert_eq!(
        chunks_of(&records, "?", &weighted_bytes_of),
        Ok(Vec::from_iter(std::iter::once(0..13)))
    );

    records[12].push('a');

    assert_eq!(
        chunks_of(&records, "?", &weighted_bytes_of),
        Ok(vec![0..12, 12..13])
    );
}

#[test]
fn the_request_limit_binds_first_for_many_short_records() {
    let question = "x".repeat(39);
    let records = records_of(1_500, "ab");

    assert_eq!(
        chunks_of(&records, &question, &bytes_of),
        Ok(vec![0..643, 643..1_286, 1_286..1_500])
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
