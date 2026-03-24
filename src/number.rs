//! Digit runs → Korean reading (short form, long Sino-Korean, native 1–2 digits).

const FULLWIDTH_DIGITS: [char; 10] = ['０', '１', '２', '３', '４', '５', '６', '７', '８', '９'];

const DIGIT_SINO: [char; 10] = ['령', '일', '이', '삼', '사', '오', '륙', '칠', '팔', '구'];

const LARGE_SCALE: [char; 16] = [
    '\0', '일', '십', '백', '천', '만', '억', '조', '\0', '\0', '\0', '한', '\0', '두', '\0', '세',
];

const SHORT_NUM_TABLE: [(char, char); 19] = [
    ('한', '\0'), // 1
    ('두', '\0'), // 2
    ('세', '\0'), // 3
    ('네', '\0'), // 4
    ('다', '섯'), // 5
    ('여', '섯'), // 6
    ('일', '곱'), // 7
    ('여', '덟'), // 8
    ('아', '홉'), // 9
    ('\0', '\0'), // 0십
    ('열', '\0'), // 10
    ('스', '물'), // 20
    ('서', '른'), // 30
    ('마', '흔'), // 40
    ('쉰', '\0'), // 50
    ('예', '순'), // 60
    ('일', '흔'), // 70
    ('여', '든'), // 80
    ('아', '흔'), // 90
];

fn digit_value(ch: char) -> Option<usize> {
    if ch.is_ascii_digit() {
        return Some((ch as u8 - b'0') as usize);
    }
    FULLWIDTH_DIGITS.iter().position(|d| *d == ch)
}

fn map_digit_sino(ch: char) -> Option<char> {
    digit_value(ch).map(|d| DIGIT_SINO[d])
}

fn convert_long_run(run: &[char]) -> Vec<char> {
    let len = run.len();
    let mut out = Vec::new();

    for (idx, ch) in run.iter().enumerate() {
        let Some(d) = digit_value(*ch) else {
            continue;
        };
        let remaining = len - idx;
        let scale = LARGE_SCALE.get(remaining).copied().unwrap_or('\0');

        if d == 1 {
            if scale != '\0' {
                out.push(scale);
            }
        } else if d != 0 {
            out.push(DIGIT_SINO[d]);
            if idx + 1 != len && scale != '\0' {
                out.push(scale);
            }
        }
    }

    out
}

fn convert_short_run(run: &[char]) -> Vec<char> {
    let len = run.len();
    let mut out = Vec::new();

    for (idx, ch) in run.iter().enumerate() {
        let Some(d) = digit_value(*ch) else {
            continue;
        };
        if d == 0 {
            continue;
        }

        let remaining = len - idx;
        let table_idx = (remaining - 1) * 10 + (d - 1);
        if let Some((a, b)) = SHORT_NUM_TABLE.get(table_idx).copied() {
            if a != '\0' {
                out.push(a);
            }
            if b != '\0' {
                out.push(b);
            }
        }
    }

    if matches!(out.last(), Some('물')) {
        *out.last_mut().expect("checked") = '무';
    }

    out
}

fn dispatch_convert(run: &[char], force_sino_short: bool) -> Vec<char> {
    match run.len() {
        0 => Vec::new(),
        1 => {
            if force_sino_short {
                run.first()
                    .and_then(|c| map_digit_sino(*c))
                    .into_iter()
                    .collect()
            } else {
                convert_short_run(run)
            }
        }
        2 => {
            if force_sino_short {
                convert_long_run(run)
            } else {
                convert_short_run(run)
            }
        }
        3..=5 => convert_long_run(run),
        _ => run.iter().filter_map(|c| map_digit_sino(*c)).collect(),
    }
}

/// Apply number conversion stage to text.
///
/// `force_sino_short` selects short-number reading for 1–2 digit runs:
/// - true: use Sino-Korean for short runs (`1 -> 일`, `12 -> 십이`)
/// - false: use native Korean forms (`1 -> 한`, `12 -> 열두`)
pub fn apply_number_conversion(text: &str, force_sino_short: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;

    let mut in_digits = false;
    let mut digit_start = 0usize;
    let mut after_decimal_marker = false;

    while i < len {
        let ch = chars[i];

        if after_decimal_marker {
            if digit_value(ch).is_some() {
                let replacement = dispatch_convert(&chars[i..i + 1], true);
                for c in replacement {
                    out.push(c);
                }
                in_digits = false;
                i += 1;
                continue;
            } else {
                after_decimal_marker = false;
            }
        }

        if digit_value(ch).is_some() {
            if !in_digits {
                digit_start = i;
                in_digits = true;
            }
            i += 1;
            continue;
        }

        // Current char is NOT a digit.
        if ch == '．' || ch == '.' {
            let next_is_digit = (i + 1 < len) && digit_value(chars[i + 1]).is_some();
            if in_digits {
                let replacement = dispatch_convert(&chars[digit_start..i], force_sino_short);
                for c in replacement {
                    out.push(c);
                }
                in_digits = false;
                out.push('쩜');
                after_decimal_marker = true;
            } else if next_is_digit {
                out.push('쩜');
                after_decimal_marker = true;
            } else {
                out.push(ch);
            }
        } else if ch == '：' || ch == ':' {
            let next_is_digit = (i + 1 < len) && digit_value(chars[i + 1]).is_some();
            if in_digits {
                let replacement = dispatch_convert(&chars[digit_start..i], force_sino_short);
                for c in replacement {
                    out.push(c);
                }
                in_digits = false;
                out.push('대');
            } else if next_is_digit {
                out.push('대');
            } else {
                out.push(ch);
            }
        } else {
            if in_digits {
                let replacement = dispatch_convert(&chars[digit_start..i], force_sino_short);
                for c in replacement {
                    out.push(c);
                }
                in_digits = false;
            }
            out.push(ch);
        }

        i += 1;
    }

    if in_digits {
        let replacement = dispatch_convert(&chars[digit_start..len], force_sino_short);
        for c in replacement {
            out.push(c);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_native_numbers() {
        assert_eq!(apply_number_conversion("1", false), "한");
        assert_eq!(apply_number_conversion("12", false), "열두");
        assert_eq!(apply_number_conversion("20", false), "스무");
    }

    #[test]
    fn test_short_forced_sino_numbers() {
        assert_eq!(apply_number_conversion("1", true), "일");
        assert_eq!(apply_number_conversion("12", true), "십이");
    }

    #[test]
    fn test_long_sino_numbers() {
        assert_eq!(apply_number_conversion("123", false), "백이십삼");
        assert_eq!(apply_number_conversion("1004", false), "천사");
    }

    #[test]
    fn test_decimal_and_colon() {
        assert_eq!(apply_number_conversion("2.5", true), "이쩜오");
        assert_eq!(apply_number_conversion("12:30", true), "십이대삼십");
    }
}
