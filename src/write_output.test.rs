use super::*;

#[test]
fn writes_columns_in_question_order_before_each_record() {
    let mut out = Vec::new();
    let columns = vec![vec![0.5, 0.25], vec![1.0, 0.125]];

    write_output(&[b"one", b"two"], &columns, b"\n", &mut out).unwrap();

    assert_eq!(out, b"0.5\t1\tone\n0.25\t0.125\ttwo\n");
}
