use crate::split_records::Separator;

#[derive(clap::Parser)]
#[command(
    name = "pyes",
    bin_name = "pyes",
    version,
    about = env!("CARGO_PKG_DESCRIPTION"),
    args_conflicts_with_subcommands = true,
    disable_help_subcommand = true
)]
pub struct Arguments {
    #[arg(
        value_name = "QUESTION",
        help = "A yes/no question asked of every record"
    )]
    questions: Vec<String>,
    #[arg(
        short = 'z',
        help = "Split records on NUL and terminate output records with NUL"
    )]
    nul: bool,
    #[arg(
        short = 'd',
        value_name = "SEPARATOR",
        conflicts_with = "nul",
        help = "Split records on this text, with \\0, \\n, \\t and \\\\ unescaped, and terminate output records with NUL"
    )]
    separator: Option<String>,
    #[command(subcommand)]
    command: Option<Subcommand>,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    #[command(about = "Store the API key read from stdin")]
    Auth,
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Score {
        questions: Vec<String>,
        separator: Separator,
    },
    Auth,
}

fn unescape(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    let mut buffer = [0; 4];

    while let Some(character) = characters.next() {
        let escaped = match (character, characters.peek()) {
            ('\\', Some('0')) => Some(b'\0'),
            ('\\', Some('n')) => Some(b'\n'),
            ('\\', Some('t')) => Some(b'\t'),
            ('\\', Some('\\')) => Some(b'\\'),
            _ => None,
        };

        match escaped {
            Some(byte) => {
                bytes.push(byte);
                characters.next();
            }
            None => bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes()),
        }
    }

    bytes
}

pub fn parse_arguments(arguments: Arguments) -> Result<Command, String> {
    if let Some(Subcommand::Auth) = arguments.command {
        return Ok(Command::Auth);
    }

    if arguments.questions.is_empty() {
        return Err("no question given".to_string());
    }

    if let Some(index) = arguments
        .questions
        .iter()
        .position(|question| question.trim().is_empty())
    {
        return Err(format!("question {} is empty", index + 1));
    }

    let separator = match (arguments.nul, arguments.separator) {
        (true, _) => Separator::Nul,
        (false, Some(text)) if text.is_empty() => {
            return Err("the -d separator is empty".to_string())
        }
        (false, Some(text)) => Separator::Text(unescape(&text)),
        (false, None) => Separator::Newline,
    };

    Ok(Command::Score {
        questions: arguments.questions,
        separator,
    })
}

#[cfg(test)]
#[path = "parse_arguments.test.rs"]
mod tests;
