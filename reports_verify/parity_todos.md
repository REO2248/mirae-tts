# Parity TODOs for full byte-exact (post 367be63)

## Truncate (FIXED)
- lib.rs:51 `truncate_last_line_char` — was always dropping final syllable, now `text.trim_end_matches(['\n','\r'])` only.
- Verified by 01_truncate_and_segment.md (Python repro) and cargo check.

## Cross-word 9w Viterbi (TODO)
- g2p.rs: `morphology_skeleton` is single-word Viterbi intra-word; cross-word 9-word window requires sentence-level windowing in `lib.rs:word_to_records` / `synthesize_bytes`.
- Blueprint: `viterbi_single_chunk` / `cands_by_start` / `MAX_CANDIDATES=214` + `conjects_verify` boundary bonus, CHUNK(60)/PROPAGATE(5).
- Evidence: fix_morphology.md + g2p_paths.md; implementation sketch in /tmp/g2p.rs.backup (needs lib.rs windowing to activate).

## Stage4 sandhi hooks (DONE @abf03ab)
- DONE: exact_stage4_linking/nasal/aspirate + PostprocessHooks + RawSecondRecord (SECOND_RECORD_SIZE 0x1dcc) integrated in g2p.rs; stage4_cross_word_sandhi delegates to stage4_cross_word_sandhi_with_hooks(PostprocessHooks::default()). Hooks return 0 without raw analyzer data (evidence-preserving no-op) and fire exact returns when present. rule_flags offsets corrected to [1]/[2]/[3] per b5c1/b5c2/b5c3.
- Evidence: fix_stage4.md 13KB exact mapping + reports_verify/g2p_stage4_backup_snapshot.rs.

## Remaining

1. lib.rs sentence window (9w) → sentence_morphology_viterbi full DP (bridge added @e42019c)

## Next step (sequential)
1. lib.rs sentence window (9w) → g2p second half Viterbi
2. g2p/tone/record stage4 PostprocessHooks wiring
3. cargo check/test → REO2248:main追積み
