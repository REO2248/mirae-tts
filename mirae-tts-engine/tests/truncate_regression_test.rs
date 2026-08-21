//! Regression: truncate_last_line_char strips trailing newlines only.
//! Design decision (2026-08-21): practical mode — the last real character of
//! the input is always synthesized. Original FUN_0042bd90 additionally drops
//! it, but that only matters for paragraph-terminated GUI documents; byte
//! parity with Test.Wav holds when the input ends with a newline, same as
//! the original's own inputs.
use mirae_tts_engine::truncate_last_line_char;

#[test]
fn no_newline_keeps_text() {
    assert_eq!(truncate_last_line_char("가"), "가");
    assert_eq!(truncate_last_line_char("가나"), "가나");
    assert_eq!(truncate_last_line_char("가나다"), "가나다");
    assert_eq!(truncate_last_line_char("Hello"), "Hello");
    assert_eq!(truncate_last_line_char(""), "");
}

#[test]
fn trailing_newline_stripped() {
    assert_eq!(truncate_last_line_char("가\n"), "가");
    assert_eq!(truncate_last_line_char("가나\n"), "가나");
    assert_eq!(truncate_last_line_char("가나다\r\n"), "가나다");
    assert_eq!(truncate_last_line_char("a\n"), "a");
    assert_eq!(truncate_last_line_char("a\r\n"), "a");
    assert_eq!(truncate_last_line_char("\n"), "");
}
