use super::*;

#[test]
fn builds_state_and_one_noul_question_per_record() {
    let records = vec!["first".to_string(), "second".to_string()];

    assert_eq!(
        request_of(&records, "Is this a rule?"),
        json!({
            "model": "jev-latest",
            "state": { "lines": { "L0": "first", "L1": "second" } },
            "questions": {
                "L0": { "type": "noul", "instructions": "For `lines.L0`, is this a rule?" },
                "L1": { "type": "noul", "instructions": "For `lines.L1`, is this a rule?" }
            }
        })
    );
}

#[test]
fn lowercases_a_non_ascii_first_letter_only() {
    assert_eq!(
        instructions_of("Épée Is Sharp?", 3),
        "For `lines.L3`, épée Is Sharp?"
    );
}
