use serde_json::{json, Map, Value};

pub const MODEL: &str = "jev-latest";

pub fn instructions_of(question: &str, index: usize) -> String {
    let mut characters = question.chars();
    let first: String = characters
        .next()
        .map(|character| character.to_lowercase().collect())
        .unwrap_or_default();

    format!("For `lines.L{index}`, {first}{}", characters.as_str())
}

pub fn request_of(records: &[String], question: &str) -> Value {
    let mut lines = Map::new();
    let mut questions = Map::new();

    for (index, record) in records.iter().enumerate() {
        lines.insert(format!("L{index}"), Value::String(record.clone()));
        questions.insert(
            format!("L{index}"),
            json!({ "type": "noul", "instructions": instructions_of(question, index) }),
        );
    }

    json!({ "model": MODEL, "state": { "lines": lines }, "questions": questions })
}

#[cfg(test)]
#[path = "request_of.test.rs"]
mod tests;
