# Oracle verification — 안녕하십니까 vs ground truth WAV

## Result: LENGTH MISMATCH (parity gap confirmed, root cause narrowed)

| | Ground truth (wine) | Rust port @ec47ffa+ |
|---|---|---|
| file | tts_reports2/ground_truth/hello_안녕하십니까_original.wav | synthesize("안녕하십니까") |
| PCM | 67,630 B = 33,815 samples (1.53 s) | 85,034 B = 42,517 samples (1.93 s) |
| ratio | | **1.257× longer** |

## Dictionary probe (KeyPad.Ebd + colligation.pkg direct)

| word | colligation | user |
|---|---|---|
| 안녕 (compound) | ✗ | ✗ |
| 녕 / 하 / 까 | ✓ | ✗ |
| 안 / 시 / 니 | ✗ | ✗ |

→ Debug trace markers `안=0x11 시=0x11 니=0x11` (fallback) are **correct per the
original greedy longest-match**: the dictionary genuinely lacks these entries.
The Viterbi lane is not active on this path (by design, goldens guard it).

## Open questions blocking closure

1. **GT capture input unknown** — filename says 안녕하십니까 but trailing
   punctuation / settings unrecorded; 293-req capture t9 is the whole article,
   not this phrase.
2. **Unit selection divergence** — 6 units both sides but avg wlen differs
   (7086 vs 5636). Need per-unit oracle (reqs+cands for THIS text) to tell
   whether VoiceInfo scoring or upstream marker/tone_class differs.

## Next actions

1. Re-capture oracle via wine with a fixed, recorded input string
   (`Future.exe.bootpatched` + trace harness exists in mirae2_re/wine_run).
2. Diff per-unit requests (phone_prev/cur/next, class, pitch, flags) between
   capture and Rust to localize the divergence (G2P vs tone vs unit_select).

Until then, byte-exact claim is limited to: constants, tables, dictionary
lookups, stage ordering, and truncate/alphabet/unit fixes — all verified
against binary/goldens. End-to-end audio parity is NOT yet proven.
