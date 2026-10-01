use super::*;

#[test]
fn drops_one_terminal_separator() {
    assert_eq!(
        split_records(b"a\nb\n", &Separator::Newline),
        vec![&b"a"[..], b"b"]
    );
    assert_eq!(
        split_records(b"a\nb", &Separator::Newline),
        vec![&b"a"[..], b"b"]
    );
}

#[test]
fn keeps_interior_empty_records() {
    assert_eq!(
        split_records(b"a\n\nb\n\n", &Separator::Newline),
        vec![&b"a"[..], b"", b"b", b""]
    );
}

#[test]
fn strips_carriage_returns_in_newline_mode() {
    assert_eq!(
        split_records(b"a\r\nb\r\n", &Separator::Newline),
        split_records(b"a\nb\n", &Separator::Newline)
    );
    assert_eq!(
        split_records(b"a\r\0b", &Separator::Nul),
        vec![&b"a\r"[..], b"b"]
    );
}

#[test]
fn newline_mode_strips_exactly_one_carriage_return() {
    assert_eq!(
        split_records(b"a\r\r\n", &Separator::Newline),
        vec![&b"a\r"[..]]
    );
}

#[test]
fn splits_on_a_multi_byte_text_separator() {
    assert_eq!(
        split_records(b"a--b---c--", &Separator::Text(b"--".to_vec())),
        vec![&b"a"[..], b"b", b"-c"]
    );
}

#[test]
fn nul_records_keep_newlines() {
    assert_eq!(
        split_records(b"a\nb\0c\n\0", &Separator::Nul),
        vec![&b"a\nb"[..], b"c\n"]
    );
}

#[test]
fn empty_input_has_no_records() {
    assert!(split_records(b"", &Separator::Newline).is_empty());
}

#[test]
fn terminator_of_each_separator() {
    assert_eq!(terminator_of(&Separator::Newline), b"\n");
    assert_eq!(terminator_of(&Separator::Nul), b"\0");
    assert_eq!(terminator_of(&Separator::Text(b"--".to_vec())), b"\0");
}
