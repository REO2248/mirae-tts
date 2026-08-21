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


## UPDATE (post 4daa8c8): pause algorithm root cause FIXED

Static disassembly of FUN_0044b880 (0x4bfb0-0x4c057) + FUN_0044b2a0 revealed:
- The original NEVER reads VoiceInfoEntry.pause (+0x18).
- Pause = FUN_0044b2a0(pause_index) where index is a min-chain of class digits
  across prev/cur/next selected units; values from engine config
  [+0xc8=1000, +0xcc=3000, +0xd0=5000, +0xd4=20000] gated by enables
  [+0xb8=0(off), +0xbc/c0/c4=1(on)] (ctor 0x4c77f).

Our port's ad-hoc `entry.pause + 1000/1500 bonuses` was replaced with exact
b2a0 semantics (@4daa8c8). 안녕하십니까 now = 21017 samples (pure unit wlen,
zero pause — correct since no sentence-end punctuation and index resolves to
disabled case 0).

Remaining GT delta (33815 vs 21017) implies the GT capture input was NOT plain
안녕하십니까 (likely longer text or trailing punctuation) — GT input must be
re-recorded with recorded input before final byte-exact claim.


## UPDATE 2: ROOT CAUSE FULLY IDENTIFIED

Cross-validation with mirae2_tts2 (byte-exact reference) revealed:

**mirae2_tts2 intentionally reproduces Future.exe's off-by-one bug** where the
WAV save driver (FUN_0042bd90) never synthesizes the last character of the last
line. Its `truncate_last_line_char` drops the final character unconditionally.

For 안녕하십니까:
- mirae2_tts2/GT: 까 dropped → 5 syllables → 13815 audio + 20000 pause = 33815
- Our port: all 6 syllables correctly synthesized → 21017 samples (no bug)

Our port's truncate fix (@34a9999) that only strips \n/\r is CORRECT for normal
text input. The GT wav was captured WITH the original's off-by-one bug.

For byte-exact comparison: input must end with a delimiter (newline/period) so
the truncate behavior doesn't affect the actual speech content.


## UPDATE 3: FULL CROSS-VALIDATION MATRIX (audit follow-up)

| Input | ours vs mirae2_tts2 | notes |
|---|---|---|
| 안녕하세요. | ✅ byte-exact (80056B) | |
| 안녕하세요.\n | ✅ byte-exact | |
| 안녕하십니까\n | ✅ byte-exact (81042B) | truncate bug reproduction verified |
| 가나다\n | ✅ byte-exact (57214B) | |
| 3.14입니다. / 3kg입니다. | ✅ byte-exact | |
| article_s09_1 (359 chars) | ✅ byte-exact (2864742B) | strongest evidence |

**Honest caveat**: mirae2_tts2 itself does NOT byte-match the wine-captured GT
WAVs at audio level (GT hello=33815 samples vs ref 40498; GT article=69.5s vs
ref 65.0s with drift accumulating from silence #30). The 281/281 proof is at
REQ (unit request stream) level, not rendered-audio level. Our port inherits
this same relationship: parity with the reference implementation is total;
parity with actual Future.exe audio output remains unproven for long inputs.

The earlier claim "byte-exact reachable by adding a delimiter" was FALSIFIED
and fixed: the truncate_last_line_char now reproduces FUN_0042bd90 exactly
(unconditional last-char drop), matching mirae2_tts2 semantics.
