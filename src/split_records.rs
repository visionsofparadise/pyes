#[derive(Debug, PartialEq)]
pub enum Separator {
    Newline,
    Nul,
    Text(Vec<u8>),
}

fn bytes_of(separator: &Separator) -> &[u8] {
    match separator {
        Separator::Newline => b"\n",
        Separator::Nul => b"\0",
        Separator::Text(text) => text,
    }
}

pub fn terminator_of(separator: &Separator) -> &'static [u8] {
    match separator {
        Separator::Newline => b"\n",
        Separator::Nul | Separator::Text(_) => b"\0",
    }
}

pub fn split_records<'a>(input: &'a [u8], separator: &Separator) -> Vec<&'a [u8]> {
    let bytes = bytes_of(separator);
    let mut records = Vec::new();
    let mut start = 0;
    let mut cursor = 0;

    while !bytes.is_empty() && cursor + bytes.len() <= input.len() {
        if &input[cursor..cursor + bytes.len()] == bytes {
            records.push(&input[start..cursor]);

            cursor += bytes.len();
            start = cursor;
        } else {
            cursor += 1;
        }
    }

    if start < input.len() {
        records.push(&input[start..]);
    }

    if *separator == Separator::Newline {
        for record in &mut records {
            if let Some(stripped) = record.strip_suffix(b"\r") {
                *record = stripped;
            }
        }
    }

    records
}

#[cfg(test)]
#[path = "split_records.test.rs"]
mod tests;
