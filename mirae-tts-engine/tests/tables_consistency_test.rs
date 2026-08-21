//! Guard: the digit-subsystem tables in `digit_tables` re-dump the hangul
//! region of `kps_tables` (which additionally covers 22 leading hanja-region
//! slots but stops one column earlier). Assert equality over the shared
//! range so a future one-sided re-dump cannot make them silently diverge.
use mirae_tts_engine::digit_tables::{KPS_COL_BASES, KPS_COL_MASKS, KPS_ROW_BOUNDS};
use mirae_tts_engine::kps_tables::{COL_MASKS, COL_STARTS, ROW_STARTS};

/// `kps_tables` prepends this many hanja-region slots before the hangul
/// columns that `digit_tables` starts at.
const HANJA_SLOTS: usize = 22;

/// `digit_tables` was dumped one column past where `kps_tables` stops.
const DIGIT_EXTRA_TAIL: usize = 1;

#[test]
fn digit_tables_match_kps_tables_overlap() {
    assert_eq!(KPS_ROW_BOUNDS, ROW_STARTS, "row bounds");
    let shared = KPS_COL_BASES.len() - DIGIT_EXTRA_TAIL;
    assert_eq!(
        &KPS_COL_BASES[..shared],
        &COL_STARTS[HANJA_SLOTS..HANJA_SLOTS + shared],
        "column bases (shared range)"
    );
    assert_eq!(
        &KPS_COL_MASKS[..shared],
        &COL_MASKS[HANJA_SLOTS..HANJA_SLOTS + shared],
        "column masks (shared range)"
    );
}
