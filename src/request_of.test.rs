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

#[test]
fn emits_records_in_input_order() {
    let records: Vec<String> = (0..12).map(|index| format!("r{index}")).collect();
    let body = serde_json::to_string(&request_of(&records, "Is it?")).unwrap();
    let state = &body[body.find("\"lines\"").unwrap()..body.find("\"questions\"").unwrap()];
    let keys: Vec<usize> = (0..12)
        .map(|index| state.find(&format!("\"L{index}\":")).unwrap())
        .collect();

    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(body.starts_with(r#"{"model":"jev-latest","state":{"lines":{"L0":"r0","L1":"r1","#));
    assert!(state.contains(r#""L9":"r9","L10":"r10","L11":"r11"}"#));
}
