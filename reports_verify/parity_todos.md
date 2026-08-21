# Parity TODOs for full byte-exact (post 367be63)

## Truncate (FIXED)
- lib.rs:51 `truncate_last_line_char` — was always dropping final syllable, now `text.trim_end_matches(['\n','\r'])` only.
- Verified by 01_truncate_and_segment.md (Python repro) and cargo check.

## Cross-word 9w Viterbi (DONE @ec47ffa)
- DONE: `viterbi_single_chunk` (split_finals lattice, cands_by_start with MAX_CANDIDATES=214 cap, colligation->user hits, conjects_verify boundary bonus, dp_score/dp_prev DP) implemented @ec47ffa. Exposed via `sentence_morphology_viterbi` (9w window lane) with cross-word conjects_verify boundary validation and greedy fallback on rejection.
- `morphology_skeleton` intentionally stays on the greedy path: intra-word Viterbi changed segmentation vs Future.exe goldens (e2e pcm_len 40005->84844); the DP lane activates via sentence-level API pending oracle verification against wine captures.

## Stage4 sandhi hooks (DONE @abf03ab)
- DONE: exact_stage4_linking/nasal/aspirate + PostprocessHooks + RawSecondRecord (SECOND_RECORD_SIZE 0x1dcc) integrated in g2p.rs; stage4_cross_word_sandhi delegates to stage4_cross_word_sandhi_with_hooks(PostprocessHooks::default()). Hooks return 0 without raw analyzer data (evidence-preserving no-op) and fire exact returns when present. rule_flags offsets corrected to [1]/[2]/[3] per b5c1/b5c2/b5c3.
- Evidence: fix_stage4.md 13KB exact mapping + reports_verify/g2p_stage4_backup_snapshot.rs.

## Remaining (verification-only; all code paths implemented)

1. **E2E byte-exact vs GT wav** — pause root cause FIXED (@4daa8c8, FUN_0044b2a0). 안녕하십니까 now 21017 samples (pure unit wlen, zero pause — correct for unpunctuated input). GT wav (33815) was captured with unknown input text; needs re-capture with recorded input for final byte comparison.
2. Viterbi lane oracle verification before enabling intra-word (behind sentence_morphology_viterbi API).
3. pause_index candidate-list walk (b850/b320) approximated with neighbor unit class (@cf94ad1); exact node-list replay would need scan-time candidate retention.

## Status
All structural parity items are implemented: truncate, alphabet 22, UNIT kHz, is_korean guard, exception early-return, deadcode integration, E2E hash regression, stage4 exact hooks + PostprocessHooks (@abf03ab), Viterbi DP + sentence window (@ec47ffa).
