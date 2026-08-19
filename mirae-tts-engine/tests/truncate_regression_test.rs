//! Regression: truncate_last_line_char must NOT drop last character when no trailing newline.
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
fn trailing_newline_stripped_only() {
    assert_eq!(truncate_last_line_char("가\n"), "가");
    assert_eq!(truncate_last_line_char("가나\n"), "가나");
    assert_eq!(truncate_last_line_char("가나다\r\n"), "가나다");
    assert_eq!(truncate_last_line_char("a\n"), "a");
    assert_eq!(truncate_last_line_char("a\r\n"), "a");
    assert_eq!(truncate_last_line_char("\n"), "");
}
