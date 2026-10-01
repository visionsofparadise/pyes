use std::io::Write;

pub fn write_output(
    records: &[&[u8]],
    columns: &[Vec<f64>],
    terminator: &[u8],
    out: &mut dyn Write,
) -> std::io::Result<()> {
    for (index, record) in records.iter().enumerate() {
        for column in columns {
            write!(out, "{}\t", column[index])?;
        }

        out.write_all(record)?;
        out.write_all(terminator)?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "write_output.test.rs"]
mod tests;
