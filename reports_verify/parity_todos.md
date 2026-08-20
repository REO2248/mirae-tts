# Parity TODOs for full byte-exact (post 367be63)

## Truncate (FIXED)
- lib.rs:51 `truncate_last_line_char` — was always dropping final syllable, now `text.trim_end_matches(['\n','\r'])` only.
- Verified by 01_truncate_and_segment.md (Python repro) and cargo check.

## Cross-word 9w Viterbi (TODO)
- g2p.rs: `morphology_skeleton` is single-word Viterbi intra-word; cross-word 9-word window requires sentence-level windowing in `lib.rs:word_to_records` / `synthesize_bytes`.
- Blueprint: `viterbi_single_chunk` / `cands_by_start` / `MAX_CANDIDATES=214` + `conjects_verify` boundary bonus, CHUNK(60)/PROPAGATE(5).
- Evidence: fix_morphology.md + g2p_paths.md; implementation sketch in /tmp/g2p.rs.backup (needs lib.rs windowing to activate).

## Stage4 sandhi hooks (TODO)
- g2p.rs: `sandhi_hook_linking/nasal/aspirate` are stubs returning 0; exact `FUN_0043f290/aaa0/f7f0` (11/2/1 returns) and `PostprocessHooks` wiring are in /tmp/g2p.rs.backup.
- Requires `WordRecord` expansion to `RawSecondRecord` (SECOND_RECORD_SIZE 0x1dcc, SECOND_*_OFFSET) plus record/tone/lib second pass.
- Evidence: fix_stage4.md 13KB exact mapping.

## Next step (sequential)
1. lib.rs sentence window (9w) → g2p second half Viterbi
2. g2p/tone/record stage4 PostprocessHooks wiring
3. cargo check/test → REO2248:main追積み
