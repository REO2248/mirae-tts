//! Regression: truncate_last_line_char must reproduce the original
//! FUN_0042bd90 behavior — the last character of the last line is NEVER
//! synthesized (t21, verified by mirae2_tts2 281/281 REQ MD5 parity).
use mirae_tts_engine::truncate_last_line_char;

#[test]
fn last_char_always_dropped() {
    assert_eq!(truncate_last_line_char("가"), "");
    assert_eq!(truncate_last_line_char("가나"), "가");
    assert_eq!(truncate_last_line_char("가나다"), "가나");
    assert_eq!(truncate_last_line_char("안녕하십니까"), "안녕하십니");
    assert_eq!(truncate_last_line_char("Hello"), "Hell");
    // multi-byte chars are dropped whole (no byte-splitting)
    assert_eq!(truncate_last_line_char("a가"), "a");
}

#[test]
fn trailing_newline_then_drop() {
    assert_eq!(truncate_last_line_char("가\n"), "");
    assert_eq!(truncate_last_line_char("가나\n"), "가");
    assert_eq!(truncate_last_line_char("가나다\r\n"), "가나");
    assert_eq!(truncate_last_line_char("a\n"), "");
    assert_eq!(truncate_last_line_char("a\r\n"), "");
    // whitespace-only/newline-only input: trim yields empty → returned
    // UNCHANGED (mirae2_tts2 `if end == 0 { return text }` branch)
    assert_eq!(truncate_last_line_char("\n"), "\n");
}

#[test]
fn empty_input_unchanged() {
    assert_eq!(truncate_last_line_char(""), "");
}
