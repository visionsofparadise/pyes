use super::*;
use clap::Parser;

fn command_of(arguments: &[&str]) -> Result<Command, String> {
    let arguments =
        Arguments::try_parse_from(std::iter::once("pyes").chain(arguments.iter().copied()))
            .map_err(|error| error.to_string())?;

    parse_arguments(arguments)
}

#[test]
fn rejects_no_question() {
    assert_eq!(command_of(&[]), Err("no question given".to_string()));
}

#[test]
fn rejects_an_empty_question_by_number() {
    assert_eq!(
        command_of(&["Is this a?", " "]),
        Err("question 2 is empty".to_string())
    );
}

#[test]
fn rejects_an_empty_separator() {
    assert_eq!(
        command_of(&["-d", "", "Is this a?"]),
        Err("the -d separator is empty".to_string())
    );
}

#[test]
fn unescapes_the_separator() {
    assert_eq!(
        command_of(&["-d", r"\0", "Is this a?"]),
        Ok(Command::Score {
            questions: vec!["Is this a?".to_string()],
            separator: Separator::Text(b"\0".to_vec()),
        })
    );
    assert_eq!(
        command_of(&["-d", r"a\n\t\\\x", "Is this a?"]),
        Ok(Command::Score {
            questions: vec!["Is this a?".to_string()],
            separator: Separator::Text(b"a\n\t\\\\x".to_vec()),
        })
    );
}

#[test]
fn auth_is_a_subcommand() {
    assert_eq!(command_of(&["auth"]), Ok(Command::Auth));
}

#[test]
fn auth_after_a_separator_is_a_question() {
    assert_eq!(
        command_of(&["--", "auth"]),
        Ok(Command::Score {
            questions: vec!["auth".to_string()],
            separator: Separator::Newline,
        })
    );
}

#[test]
fn help_is_a_question() {
    assert_eq!(
        command_of(&["help"]),
        Ok(Command::Score {
            questions: vec!["help".to_string()],
            separator: Separator::Newline,
        })
    );
}
