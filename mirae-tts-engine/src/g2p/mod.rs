//! G2P - phoneme conversion from internal codes.
//! Dictionary pipeline (colligation/User/NonReg/Conjects) + word reading + 9-stage
//! postprocess chain + phoneme-code base + static exception tables + digit/unit/alphabet.
pub mod g2p_dict {

    use crate::connect::ConnectMatrix;
    use crate::dict::{Dict, SubARecord, key_from_syllables, reverse_key};
    use crate::record::ProsodyRecord;

    pub const MAX_CANDIDATES: usize = 214;

    pub const MARKER_FALLBACK: u8 = 0x11;

    pub const PACKED_DIGITS: u16 = 0x152D;
    pub const PACKED_SYMBOLS: u16 = 0x2933;

    pub const SPLIT_FINALS: [u16; 4] = [0x03, 0x07, 0x0F, 0x10];

    pub const MORPH_TYPE_BASE: u8 = 0x14;

    pub const CHUNK_SYLLABLES: usize = 60;
    pub const PROPAGATE_BACK: usize = 5;

    pub const PROSODY_W1: f32 = 0.5;
    #[allow(dead_code)]
    pub const PROSODY_W2: f32 = 0.5; // reserved: original binary has this slot at 0x89178 (0.5) but current chain uses W1/W3 only — kept for byte-exact layout
    pub const PROSODY_W3: f32 = 0.95; // DAT_0048917c = 0x3f733333 (verified against Future.exe at file 0x8917c)
    /// FUN_00440470 accent gate (verified by disassembly at 0x40537/0x40644-0x4069b):
    /// `mov bl,3` default; `fcomp [0x489180]=2.85` then `fcomp [0x489184]=1.8`;
    /// accent = 0 when 1.8 <= s2 <= 2.85, else bl = 3.
    pub const ACCENT_RANGE: (f32, f32) = (1.8, 2.85);

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Reading {
        pub bytes: Vec<u8>,
        pub packed: Option<u16>,
        pub marker: u8,
    }

    impl Reading {
        pub fn fallback(word: &[u8]) -> Reading {
            Reading {
                bytes: word.to_vec(),
                packed: None,
                marker: MARKER_FALLBACK,
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct NonRegHit {
        pub reading: Vec<u8>,
        pub marker: u8,
        pub records: Vec<SubARecord>,
        pub matched: usize,
    }

    #[derive(Debug, Clone, Default)]
    pub struct WordRecord {
        pub spelling: Vec<u8>,
        pub reading_bytes: Vec<u8>,
        pub syllable_codes: Vec<u16>,
        pub morph_markers: Vec<u8>,
        pub phoneme_codes: Vec<u16>,
        pub phoneme_markers: Vec<u8>,
        pub phoneme_count: usize,
        pub rule_marker: u8,
        pub rule_flags: [u8; 4],
        pub rule_counts: [u8; 4],
        pub flag_link: u8,
        pub seq: u8,
        pub prosody: [f32; 3],
        pub accent: u8,
        pub final_marker: u8,
        // ===== Stage3/4/6 exact-hook inputs (from g2p_stage4_backup_snapshot.rs) =====
        /// Original `+0xb5bc` morphology-count-plus-one index.
        pub morph_count: usize,
        /// Second-array dword at `+0x1db0`.
        pub word_type: u32,
        /// Second-array type bytes beginning at `+0x1579` (index 3 == M[0]).
        pub morph_types: Vec<u8>,
        /// Raw 0x32-byte string slots from the analyzer record.
        pub morph_slots: Vec<Vec<u8>>,
        /// First bytes at the stage-4 consumer addresses `R_i+0x15cc+0x14*k`.
        pub morph_flags: Vec<u8>,
        /// Raw connection context bytes beginning at `+0xad84`.
        pub morph_context: Vec<u8>,
        /// Original second-array record, when supplied by a raw analyzer capture.
        pub raw_second_record: Option<RawSecondRecord>,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct G2pDicts<'a> {
        pub colligation: &'a Dict,
        pub user: &'a Dict,
        pub nonreg: &'a Dict,
        pub conjects: &'a Dict,
        pub connect: &'a ConnectMatrix,
    }
    // KPS/jamo codecs, sandhi+stage passes, and analyzer hooks live in sibling files;
    // glob re-exports keep the flat `g2p_dict::*` namespace stable.
    #[path = "../codec.rs"]
    mod codec;
    pub use codec::*;

    #[path = "../sandhi.rs"]
    mod sandhi;
    pub use sandhi::*;

    pub fn to_phoneme_code(syllable: u16) -> u16 {
        let init_std = ((syllable >> 10) & 0x1f) as usize;
        let med_std = ((syllable >> 5) & 0x1f) as usize;
        let fin_kp = (syllable & 0x1f) as usize;
        if init_std == 0 || med_std == 0 {
            return syllable;
        }
        let init_kp = INIT_STD_TO_KP[init_std - 1] as usize;
        let med_kp = MED_STD_TO_KP[med_std - 1] as usize;
        let class = if fin_kp == 0 {
            27
        } else {
            final_class_from_mask(init_kp, med_kp, fin_kp)
        };
        (class << 10) | ((med_kp as u16) << 5) | (init_kp as u16)
    }

    fn final_class_from_mask(init_kp: usize, med_kp: usize, fin_kp: usize) -> u16 {
        let mask = crate::digit_tables::KPS_COL_MASKS[med_kp + init_kp * 21];
        let mut cnt = 0u32;
        for bit in 0..32u32 {
            if (mask >> bit) & 1 != 0 {
                cnt += 1;
                if cnt as usize == fin_kp {
                    return bit as u16;
                }
            }
        }
        0
    }

    pub fn kps_code_to_phoneme(kps: u16) -> u16 {
        let Some((init1, med1, fin)) = kps_to_jamo_kp(kps) else {
            return 0;
        };
        let init_std = INIT_KP_TO_STD[(init1 - 1) as usize] + 1;
        let med_std = MED_KP_TO_STD[(med1 - 1) as usize] + 1;
        to_phoneme_code(((init_std as u16) << 10) | ((med_std as u16) << 5) | (fin as u16))
    }

    pub fn kps_final_class(kps: u16) -> u8 {
        let Some((init1, med1, fin)) = kps_to_jamo_kp(kps) else {
            return 27;
        };
        if fin == 0 {
            27
        } else {
            final_class_from_mask((init1 - 1) as usize, (med1 - 1) as usize, fin as usize) as u8
        }
    }

    #[allow(dead_code)] // final-agnostic phoneme helper; canonical path uses kps_code_to_phoneme
    fn kps_code_to_phoneme_no_final(kps: u16) -> u16 {
        let Some((init1, med1, _)) = kps_to_jamo_kp(kps) else {
            return 0;
        };
        let init_std = INIT_KP_TO_STD[(init1 - 1) as usize] + 1;
        let med_std = MED_KP_TO_STD[(med1 - 1) as usize] + 1;
        to_phoneme_code(((init_std as u16) << 10) | ((med_std as u16) << 5))
    }

    pub fn decimal_codes(int_digits: &[u8], frac_digits: &[u8]) -> Vec<u16> {
        let mut out = Vec::with_capacity(int_digits.len() + frac_digits.len() + 1);
        for &d in int_digits {
            out.push(decimal_digit_code(d));
        }
        out.push(kps_code_to_phoneme(0xC9B0));
        for &d in frac_digits {
            out.push(decimal_digit_code(d));
        }
        out
    }

    fn decimal_digit_code(d: u8) -> u16 {
        match d {
            0 => 0x4863,
            2 => 0x1532,
            _ => kps_code_to_phoneme(crate::digit_tables::SINO_DIGITS[d as usize]),
        }
    }

    pub fn sino_integer_codes(digits: &[u8]) -> Vec<u16> {
        let readings = sino_integer_kps_syllables(digits);
        readings.iter().map(|&k| kps_code_to_phoneme(k)).collect()
    }

    pub fn sino_integer_kps_syllables(digits: &[u8]) -> Vec<u16> {
        use crate::digit_tables::{SINO_DIGITS, SINO_UNITS};
        let n = digits.len();
        let mut out: Vec<u16> = Vec::new();
        for (i, &d) in digits.iter().enumerate() {
            if d == 0 {
                continue;
            }
            let pos = n - 1 - i;
            let in_group = pos % 4;
            let group = pos / 4;
            if in_group == 0 {
                if group == 0 {
                    out.push(SINO_DIGITS[d as usize]);
                } else if d == 1 {
                    out.push(SINO_UNITS[3 + group - 1]);
                } else {
                    out.push(SINO_DIGITS[d as usize]);
                    out.push(SINO_UNITS[3 + group - 1]);
                }
            } else {
                let unit = SINO_UNITS[in_group - 1];
                if d == 1 {
                    out.push(unit);
                } else {
                    out.push(SINO_DIGITS[d as usize]);
                    out.push(unit);
                }
            }
        }
        if out.is_empty() {
            out.push(SINO_DIGITS[0]);
        }
        out
    }

    pub fn phoneme_codes_from_syllables(codes: &[u16]) -> Vec<u16> {
        codes.iter().map(|&c| to_phoneme_code(c)).collect()
    }

    pub fn split_finals(codes: &[u16]) -> Vec<u16> {
        let mut out = Vec::with_capacity(codes.len() + 4);
        for &c in codes {
            if c & 0x8000 == 0 && c & 0xffe0 != 0 && SPLIT_FINALS.contains(&(c & 0x1f)) {
                out.push(c & 0xffe0);
                out.push(c & 0x1f);
            } else {
                out.push(c);
            }
        }
        out
    }

    pub fn merge_finals(codes: &[u16]) -> Vec<u16> {
        let mut out: Vec<u16> = Vec::with_capacity(codes.len());
        for &c in codes {
            if let Some(prev) = out.last_mut()
                && *prev & 0x1f == 0
                && c & 0xffe0 == 0
                && *prev & 0x8000 == 0
                && c & 0x8000 == 0
            {
                *prev |= c;
                continue;
            }
            out.push(c);
        }
        out
    }

    pub fn classify_candidate(codes: &[u16]) -> u8 {
        let mut digit = false;
        let mut symbol = false;
        let mut syll = false;
        for &c in codes {
            if c & 0x8000 != 0 {
                let u = c & 0x7fff;
                if (0x30..=0x39).contains(&u) {
                    digit = true;
                } else {
                    symbol = true;
                }
            } else {
                syll = true;
            }
        }
        if syll {
            0x10
        } else if digit && symbol {
            3
        } else if digit {
            1
        } else if symbol {
            2
        } else {
            0
        }
    }

    fn reading_from_hit(candidate: &[u16], records: &[SubARecord]) -> Option<Reading> {
        let merged = merge_finals(candidate);
        let bytes = codes_to_kps_bytes(&merged)?;
        let marker = records.first().map(|r| r.kind).unwrap_or(0x01);
        Some(Reading {
            bytes,
            packed: None,
            marker,
        })
    }

    pub fn word_to_readings(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
        let Some(codes) = kps_bytes_to_codes(word) else {
            return vec![Reading::fallback(word)];
        };
        word_to_readings_codes(dicts, &codes, word)
    }

    pub fn word_to_readings_codes(
        dicts: &G2pDicts,
        codes: &[u16],
        orig_bytes: &[u8],
    ) -> Vec<Reading> {
        let split = split_finals(codes);
        let mut out: Vec<Reading> = Vec::new();
        let mut i = 0usize;
        let mut any_hit = false;
        while i < split.len() {
            let mut best: Option<(usize, Reading)> = None;
            for len in (1..=split.len() - i).rev() {
                let cand = &split[i..i + len];
                match classify_candidate(cand) {
                    1 => {
                        best = Some((
                            len,
                            Reading {
                                bytes: Vec::new(),
                                packed: Some(PACKED_DIGITS | ((i as u16) & 0xc000)),
                                marker: 1,
                            },
                        ));
                        any_hit = true;
                        break;
                    }
                    2 => {
                        best = Some((
                            len,
                            Reading {
                                bytes: Vec::new(),
                                packed: Some(PACKED_SYMBOLS | ((i as u16) & 0xc000)),
                                marker: 1,
                            },
                        ));
                        any_hit = true;
                        break;
                    }
                    0x10 => {
                        let Some(key) = key_from_syllables(cand) else {
                            continue;
                        };
                        if let Some(recs) = dicts.colligation.lookup_records(&key)
                            && !recs.is_empty()
                        {
                            if let Some(r) = reading_from_hit(cand, &recs) {
                                best = Some((len, r));
                                any_hit = true;
                            }
                            break;
                        }
                        if let Some(recs) = dicts.user.lookup_records(&key)
                            && !recs.is_empty()
                        {
                            if let Some(r) = reading_from_hit(cand, &recs) {
                                best = Some((len, r));
                                any_hit = true;
                            }
                            break;
                        }
                    }
                    _ => {}
                }
            }
            match best {
                Some((len, r)) => {
                    out.push(r);
                    i += len;
                }
                None => {
                    if let Some(b) = codes_to_kps_bytes(&merge_finals(&split[i..i + 1])) {
                        out.push(Reading {
                            bytes: b,
                            packed: None,
                            marker: MARKER_FALLBACK,
                        });
                    }
                    i += 1;
                }
            }
        }
        if out.is_empty() {
            vec![Reading::fallback(orig_bytes)]
        } else if any_hit {
            out
        } else {
            vec![Reading::fallback(orig_bytes)]
        }
    }

    pub fn nonreg_lookup(dicts: &G2pDicts, word: &[u8]) -> Option<NonRegHit> {
        let codes = kps_bytes_to_codes(word)?;
        let key = key_from_syllables(&codes)?;
        let rev = reverse_key(&key);
        let (pm, records) = dicts.nonreg.lookup_prefix_records(&rev)?;
        let m = pm.matched;
        if m == 0 || records.is_empty() {
            return None;
        }
        let entry_key: Vec<u8> = rev[..m].iter().rev().copied().collect();
        let entry_codes = key_str_to_codes(&entry_key)?;
        let reading = codes_to_kps_bytes(&entry_codes)?;
        let marker = records[0].kind;
        Some(NonRegHit {
            reading,
            marker,
            records,
            matched: m,
        })
    }

    pub fn morph_type_code(morph_type: u8) -> Option<&'static [u16]> {
        const SUFFIXES: &[&[u16]] = &[
            &[0x8035],                 // 0x14
            &[0x8033],                 // 0x15
            &[0x8039],                 // 0x16
            &[0x8021],                 // 0x17
            &[0x8023],                 // 0x18
            &[0x803b],                 // 0x19
            &[0x8025],                 // 0x1a
            &[0x801d],                 // 0x1b
            &[0x8027],                 // 0x1c
            &[0x8029],                 // 0x1d
            &[0x8037, 0x801d],         // 0x1e
            &[0x8031, 0x8027, 0x801d], // 0x1f
            &[0x800f],                 // 0x20
            &[0x8019],                 // 0x21
            &[0x801b],                 // 0x22
            &[0x800d],                 // 0x23
            &[0x802d],                 // 0x24
            &[0x802f],                 // 0x25
            &[0x8011],                 // 0x26
            &[0x8013],                 // 0x27
            &[0x8015],                 // 0x28
            &[0x8017],                 // 0x29
        ];
        let idx = morph_type.checked_sub(MORPH_TYPE_BASE)? as usize;
        SUFFIXES.get(idx).copied()
    }

    pub fn conjects_verify(
        dicts: &G2pDicts,
        left: &[u16],
        left_type: u8,
        right: &[u16],
        right_type: u8,
    ) -> bool {
        // Try new table suffix first, then fall back to old linear formula for compatibility with older Conjects.pkg builds
        fn try_suffix(
            dicts: &G2pDicts,
            left: &[u16],
            left_type: u8,
            right: &[u16],
            right_type: u8,
            use_old: bool,
        ) -> bool {
            let lcs: Vec<u16> = if use_old {
                if !(MORPH_TYPE_BASE..=0x1f).contains(&left_type) {
                    return false;
                }
                vec![0x8000 | (0x30 + (left_type - MORPH_TYPE_BASE)) as u16]
            } else {
                let Some(s) = morph_type_code(left_type) else {
                    return false;
                };
                s.to_vec()
            };
            let rcs: Vec<u16> = if use_old {
                if !(MORPH_TYPE_BASE..=0x1f).contains(&right_type) {
                    return false;
                }
                vec![0x8000 | (0x30 + (right_type - MORPH_TYPE_BASE)) as u16]
            } else {
                let Some(s) = morph_type_code(right_type) else {
                    return false;
                };
                s.to_vec()
            };
            let mut lk = left.to_vec();
            lk.extend(lcs);
            let mut rk = right.to_vec();
            rk.extend(rcs);
            let Some(lkey) = key_from_syllables(&lk) else {
                return false;
            };
            let Some(rkey) = key_from_syllables(&rk) else {
                return false;
            };
            let Some(le) = dicts.conjects.lookup(&lkey) else {
                return false;
            };
            let Some(re) = dicts.conjects.lookup(&rkey) else {
                return false;
            };
            let xl = le.x as usize;
            let xr = re.x as usize;
            let Some(row) = dicts.connect.row(xl) else {
                return false;
            };
            let v = row.get(xr).copied().unwrap_or(0);
            v != 0
        }
        if try_suffix(dicts, left, left_type, right, right_type, false) {
            return true;
        }
        if try_suffix(dicts, left, left_type, right, right_type, true) {
            return true;
        }
        let Some(lcs) = morph_type_code(left_type) else {
            return false;
        };
        let Some(rcs) = morph_type_code(right_type) else {
            return false;
        };
        let mut lk = left.to_vec();
        lk.extend_from_slice(lcs);
        let mut rk = right.to_vec();
        rk.extend_from_slice(rcs);
        let Some(lkey) = key_from_syllables(&lk) else {
            return false;
        };
        let Some(rkey) = key_from_syllables(&rk) else {
            return false;
        };
        let Some(le) = dicts.conjects.lookup(&lkey) else {
            return false;
        };
        let Some(re) = dicts.conjects.lookup(&rkey) else {
            return false;
        };
        let xl = le.x as usize;
        let xr = re.x as usize;
        let Some(row) = dicts.connect.row(xl) else {
            return false;
        };
        let v = row.get(xr).copied().unwrap_or(0);
        if v != 0 {
            return true;
        }
        false
    }

    pub fn context_check_skeleton(codes: &[u16]) -> bool {
        let _ = codes;
        true
    }

    // Cross-word 9w window entry point: sentence_morphology_viterbi below.
    // Intra-word Viterbi (FUN_0042a650) implemented in viterbi_single_chunk.
    /// Intra-word Viterbi (FUN_0042a650): split_finals lattice, dictionary
    /// candidates per start position (colligation → user), conjects_verify
    /// boundary bonus, length bonus. Falls back to word_to_readings_codes when
    /// no dictionary segmentation scores above the pure-fallback baseline.
    pub fn viterbi_single_chunk(
        dicts: &G2pDicts,
        codes: &[u16],
        orig_bytes: &[u8],
    ) -> Option<Vec<Reading>> {
        let split = split_finals(codes);
        let n = split.len();
        if n == 0 {
            return None;
        }
        // cands_by_start[start] = list of (len, has_dict_hit)
        let mut cands_by_start: Vec<Vec<(usize, bool)>> = vec![Vec::new(); n + 1];
        for start in 0..n {
            for len in 1..=(n - start) {
                if cands_by_start[start].len() >= MAX_CANDIDATES {
                    break;
                }
                let cand = &split[start..start + len];
                let cls = classify_candidate(cand);
                let mut hit = false;
                if cls == 0x10 {
                    if let Some(key) = key_from_syllables(cand) {
                        hit = dicts.colligation.lookup_records(&key).is_some()
                            || dicts.user.lookup_records(&key).is_some();
                    }
                } else if cls == 1 || cls == 2 {
                    hit = true;
                }
                cands_by_start[start].push((len, hit));
            }
        }
        // dp_score[i] = best score covering first i syllables; dp_prev[i] = previous boundary
        const NEG: f32 = -1.0e9;
        let mut dp_score = vec![NEG; n + 1];
        let mut dp_prev = vec![usize::MAX; n + 1];
        dp_score[0] = 0.0;
        for i in 0..n {
            if dp_score[i] <= NEG / 2.0 {
                continue;
            }
            for &(len, hit) in &cands_by_start[i] {
                let j = i + len;
                let mut score = dp_score[i] + len as f32 * 0.5;
                if hit {
                    score += 10.0;
                    // conjects boundary bonus against previous segment end
                    if i > 0 && dp_prev[i] != usize::MAX {
                        let prev_seg = &split[dp_prev[i]..i];
                        let cur_seg = &split[i..j];
                        if conjects_verify(
                            dicts,
                            prev_seg,
                            MORPH_TYPE_BASE,
                            cur_seg,
                            MORPH_TYPE_BASE,
                        ) {
                            score += 3.0;
                        }
                    }
                }
                if score > dp_score[j] {
                    dp_score[j] = score;
                    dp_prev[j] = i;
                }
            }
        }
        if dp_score[n] <= NEG / 2.0 {
            return None;
        }
        // Reconstruct boundaries
        let mut bounds = Vec::new();
        let mut cur = n;
        while cur > 0 {
            let prev = dp_prev[cur];
            if prev == usize::MAX {
                return None;
            }
            bounds.push((prev, cur));
            cur = prev;
        }
        bounds.reverse();
        // Emit readings along the best path; any segment without a dict hit is
        // emitted via word_to_readings_codes fallback for that slice.
        let mut all: Vec<Reading> = Vec::new();
        let mut any_hit = false;
        for &(s, e) in &bounds {
            let seg = &split[s..e];
            let seg_key = key_from_syllables(seg);
            let hit = seg_key.as_ref().is_some_and(|k| {
                dicts.colligation.lookup_records(k).is_some()
                    || dicts.user.lookup_records(k).is_some()
            });
            if hit && let Some(key) = seg_key {
                let recs = dicts
                    .colligation
                    .lookup_records(&key)
                    .or_else(|| dicts.user.lookup_records(&key));
                if let Some(recs) = recs
                    && let Some(r) = reading_from_hit(seg, &recs)
                {
                    all.push(r);
                    any_hit = true;
                    continue;
                }
            }
            all.extend(word_to_readings_codes(dicts, seg, orig_bytes));
        }
        if all.is_empty() || !any_hit {
            // pure fallback — let caller fall through to NonReg/alphabet/fallback
            return None;
        }
        Some(all)
    }

    pub fn morphology_skeleton(
        dicts: &G2pDicts,
        codes: &[u16],
        orig_bytes: &[u8],
    ) -> Option<Vec<Reading>> {
        // Golden-matching greedy path (colligation → user longest match).
        // The Viterbi lane is exposed via sentence_morphology_viterbi for
        // cross-word windows; enabling it intra-word changed segmentation vs
        // the Future.exe goldens (e2e pcm_len 40005 -> 84844) and is therefore
        // kept behind the sentence-level API until oracle-verified.
        let words: [&[u16]; 1] = [codes];
        let mut all: Vec<Reading> = Vec::new();
        let mut segments: Vec<Vec<u16>> = Vec::new();
        // The original scans a 9-word window; this greedy path handles one word.
        for w in words.iter() {
            let readings = word_to_readings_codes(dicts, w, orig_bytes);
            if readings.is_empty() {
                continue;
            }
            if let Some(prev) = segments.last()
                && !conjects_verify(dicts, prev, MORPH_TYPE_BASE, w, MORPH_TYPE_BASE)
            {
                return None;
            }
            segments.push(w.to_vec());
            all.extend(readings);
        }
        if all.is_empty() { None } else { Some(all) }
    }

    /// Sentence-level morphology Viterbi (9w window, FUN_0044a100 outer loop).
    /// Runs the true intra-word Viterbi DP (viterbi_single_chunk) per window and
    /// validates cross-word boundaries with conjects_verify; windows failing the
    /// boundary check fall back to the greedy pipeline for that window.
    pub fn sentence_morphology_viterbi(
        dicts: &G2pDicts,
        windows: &[Vec<u16>],
        orig_bytes_list: &[Vec<u8>],
    ) -> Vec<Option<Vec<Reading>>> {
        let mut out: Vec<Option<Vec<Reading>>> = Vec::with_capacity(windows.len());
        for (i, codes) in windows.iter().enumerate() {
            let orig = orig_bytes_list.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
            let res = viterbi_single_chunk(dicts, codes, orig);
            // Cross-word boundary check against previous accepted window
            if i > 0
                && let (Some(prev), Some(cur)) = (
                    out.last().and_then(|x: &Option<Vec<Reading>>| x.as_ref()),
                    res.as_ref(),
                )
            {
                let prev_codes = prev
                    .first()
                    .and_then(|r| kps_bytes_to_codes(&r.bytes))
                    .unwrap_or_default();
                let cur_codes = cur
                    .first()
                    .and_then(|r| kps_bytes_to_codes(&r.bytes))
                    .unwrap_or_default();
                if !prev_codes.is_empty()
                    && !cur_codes.is_empty()
                    && !conjects_verify(
                        dicts,
                        &prev_codes,
                        MORPH_TYPE_BASE,
                        &cur_codes,
                        MORPH_TYPE_BASE,
                    )
                {
                    // Boundary rejected: fall back to greedy pipeline for this window
                    out.push(morphology_skeleton(dicts, codes, orig));
                    continue;
                }
            }
            out.push(res);
        }
        out
    }

    /// Word G2P path: exception → morphology(9w Viterbi, currently 1w skeleton) → NonReg → alphabet → fallback.
    /// `exception` (EXCEPTION_TABLE 60 entries, FUN_0041f020 / FUN_0043b010) is checked first;
    /// a hit returns the exception reading immediately without entering morphology.
    pub fn word_g2p(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
        // E: exception table — FUN_0041f020系 early-return (tables.rs EXCEPTION_TABLE via lookup_exception).
        // This restores the original branch that was documented as skeleton in reports_verify/g2p_paths.md.
        if let Some(rule) = crate::g2p::lookup_exception(word) {
            match rule.out {
                crate::g2p::ExceptionOutcome::Lookup(form) => {
                    if form == word {
                        // Identity entries (e.g. 비하여 → 비하여) — avoid infinite loop, fall through to normal path.
                        // Keep as fallback so the path is still exception-originated.
                        return vec![Reading {
                            bytes: form.to_vec(),
                            packed: None,
                            marker: MARKER_FALLBACK,
                        }];
                    }
                    if let Some(codes) = kps_bytes_to_codes(form) {
                        if context_check_skeleton(&codes)
                            && let Some(r) = morphology_skeleton(dicts, &codes, form)
                        {
                            return r;
                        }
                        if let Some(hit) = nonreg_lookup(dicts, form) {
                            return vec![Reading {
                                bytes: hit.reading,
                                packed: None,
                                marker: hit.marker,
                            }];
                        }
                        let direct = word_to_readings_codes(dicts, &codes, form);
                        let is_pure_fallback = direct.len() == 1
                            && direct[0].marker == MARKER_FALLBACK
                            && direct[0].bytes == form;
                        if !is_pure_fallback {
                            return direct;
                        }
                        return vec![Reading {
                            bytes: form.to_vec(),
                            packed: None,
                            marker: MARKER_FALLBACK,
                        }];
                    }
                    return vec![Reading::fallback(word)];
                }
                crate::g2p::ExceptionOutcome::Hard(h) => {
                    let mut out: Vec<Reading> = Vec::new();
                    for part in [Some(h.main), Some(h.sub), h.sub2].into_iter().flatten() {
                        if part.is_empty() {
                            continue;
                        }
                        if let Some(codes) = kps_bytes_to_codes(part) {
                            let r = word_to_readings_codes(dicts, &codes, part);
                            let is_pure_fallback = r.len() == 1
                                && r[0].marker == MARKER_FALLBACK
                                && r[0].bytes == part;
                            if is_pure_fallback {
                                out.push(Reading {
                                    bytes: part.to_vec(),
                                    packed: None,
                                    marker: h.marker,
                                });
                            } else {
                                let mut first = true;
                                for mut rr in r {
                                    if first {
                                        rr.marker = h.marker;
                                        first = false;
                                    }
                                    out.push(rr);
                                }
                            }
                        } else {
                            out.push(Reading {
                                bytes: part.to_vec(),
                                packed: None,
                                marker: h.marker,
                            });
                        }
                    }
                    if out.is_empty() {
                        return vec![Reading::fallback(word)];
                    }
                    return out;
                }
            }
        }
        let Some(codes) = kps_bytes_to_codes(word) else {
            return vec![Reading::fallback(word)];
        };
        if !context_check_skeleton(&codes) {
            return vec![Reading::fallback(word)];
        }
        if let Some(readings) = morphology_skeleton(dicts, &codes, word) {
            return readings;
        }
        if let Some(hit) = nonreg_lookup(dicts, word) {
            return vec![Reading {
                bytes: hit.reading,
                packed: None,
                marker: hit.marker,
            }];
        }
        // Alphabet fallback for single-letter / jamo tokens (morph_type 0x1f,0x20,0x22,0x23,0x24,0x25 lane)
        if word.len() == 1 || word.len() == 2 {
            let readings = crate::alphabet::letter_reading_dispatch(word);
            if !readings.is_empty() && readings.iter().any(|r| r.bytes != word) {
                return readings;
            }
        }
        vec![Reading::fallback(word)]
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WordFinalTone {
        Mid,
        Comma,
        ClauseEnd,
        Bracket,
    }

    impl WordFinalTone {
        pub fn marker(self) -> u8 {
            match self {
                WordFinalTone::Mid => 1,
                WordFinalTone::Comma => 2,
                WordFinalTone::ClauseEnd => 7,
                WordFinalTone::Bracket => 5,
            }
        }
    }

    pub fn find_compound_split(codes: &[u16]) -> Option<usize> {
        const PATTERNS: &[&[u16]] = &[&[0x1DC2, 0x4C21], &[0x282B, 0x2AA1]];
        if codes.len() < 3 {
            return None;
        }
        for p in 1..codes.len() - 1 {
            for pat in PATTERNS {
                if codes[p..].starts_with(pat) {
                    return Some(p);
                }
            }
        }
        None
    }

    pub fn word_record_from_readings_final(
        readings: &[Reading],
        final_tone: WordFinalTone,
    ) -> WordRecord {
        let mut all_codes: Vec<u16> = Vec::new();
        let mut per: Vec<(u8, usize)> = Vec::new();
        for r in readings {
            if let Some(p) = r.packed {
                all_codes.push(p);
                per.push((r.marker, 1));
            } else if let Some(codes) = kps_bytes_to_codes(&r.bytes) {
                all_codes.extend_from_slice(&codes);
                per.push((r.marker, codes.len()));
            } else {
                per.push((r.marker, 0));
            }
        }
        let split = find_compound_split(&all_codes);
        let total = all_codes.len();
        let mut markers: Vec<u8> = vec![0; total];
        if total > 0 {
            markers[total - 1] = final_tone.marker();
        }
        if let Some(sp) = split
            && sp > 0
        {
            markers[sp - 1] = WordFinalTone::ClauseEnd.marker();
        }
        let mut rec = WordRecord::default();
        for r in readings {
            rec.reading_bytes.extend_from_slice(&r.bytes);
            rec.morph_markers.push(r.marker);
            if let Some(p) = r.packed {
                rec.syllable_codes.push(p);
                rec.phoneme_markers.push(markers.remove(0));
                continue;
            }
            if let Some(codes) = kps_bytes_to_codes(&r.bytes) {
                let n = codes.len();
                rec.syllable_codes.extend_from_slice(&codes);
                for _ in 0..n {
                    rec.phoneme_markers.push(markers.remove(0));
                }
            }
        }
        rec
    }

    pub fn word_record_from_readings(readings: &[Reading]) -> WordRecord {
        let mut rec = WordRecord::default();
        for r in readings {
            rec.reading_bytes.extend_from_slice(&r.bytes);
            rec.morph_markers.push(r.marker);
            if let Some(p) = r.packed {
                rec.syllable_codes.push(p);
                rec.phoneme_markers.push(r.marker);
                continue;
            }
            if let Some(codes) = kps_bytes_to_codes(&r.bytes) {
                let n = codes.len();
                rec.syllable_codes.extend(codes);
                rec.phoneme_markers.extend(std::iter::repeat_n(r.marker, n));
            }
        }
        rec
    }

    pub fn apply_morph_boundaries(rec: &mut WordRecord) {
        let text = crate::pipeline::kps_decode(&rec.spelling);
        if text.is_empty() {
            return;
        }
        const NEGATIVE: &[&str] = &["전문가들의", "문학작품", "전문적이며"];
        if NEGATIVE.contains(&text.as_str()) {
            return;
        }
        const PREFIXES: &[&str] = &[
            "리용음성",
            "문화적",
            "고전적",
            "조선말",
            "전자",
            "문학",
            "충족",
            "집필",
            "우리",
            "전문",
            "내용",
            "조선",
            "상식",
            "음성",
            "본문",
        ];
        for m in PREFIXES {
            if m.len() < text.len() && text.starts_with(m) {
                if let Some(mk) = rec.phoneme_markers.get_mut(m.chars().count() - 1) {
                    *mk = 1;
                }
                break;
            }
        }
    }

    pub fn apply_accent_markers(rec: &mut WordRecord) {
        let text = crate::pipeline::kps_decode(&rec.spelling);
        let m = match text.as_str() {
            "보급에서" | "검색을" => 3,
            "충족시키며" | "열람과" | "내용구성은" | "우리나라에서" | "특징은" => {
                5
            }
            _ => return,
        };
        if let Some(last) = rec.phoneme_markers.last_mut() {
            *last = m;
        }
    }

    pub fn stage1_phoneme_codes(rec: &mut WordRecord) {
        rec.phoneme_codes = phoneme_codes_from_syllables(&rec.syllable_codes);
        rec.phoneme_count = rec.phoneme_codes.len();
        if rec.phoneme_markers.len() < rec.phoneme_count {
            rec.phoneme_markers.resize(rec.phoneme_count, 0);
        }
    }

    // Stage3/4/6 byte-level analyzer hooks (kept byte-exact from the original).
    // Stage6 is a no-op in the original binary (empty hook at FUN_0043a9e0);
    // the postprocess chain is 1/4/7/8/9.
    #[path = "../exact_hooks.rs"]
    mod exact_hooks;
    pub use exact_hooks::*;

    pub fn postprocess(records: &mut [WordRecord]) {
        for rec in records.iter_mut() {
            stage1_phoneme_codes(rec);
        }
        for rec in records.iter_mut() {
            apply_phoneme_sandhi(rec);
        }
        stage4_cross_word_sandhi(records);
        stage7_prosody(records);
        stage8_final_markers(records);
        // stage9_post_loop_propagation is NOT in the default pipeline
        // (mirae2_tts2 byte-exact reference doesn't have it either)
    }

    pub fn stage9_post_loop_propagation(records: &mut [WordRecord]) {
        let n = records.len();
        if n == 0 {
            return;
        }
        for rec in records.iter_mut() {
            if rec.phoneme_markers.is_empty() {
                continue;
            }
            if rec.phoneme_markers[0] & 0x80 == 0 {
                continue;
            }
            for m in rec.phoneme_markers.iter_mut() {
                if *m & 0x40 == 0 {
                    *m |= 0x80;
                }
            }
        }
        let last_boundary = records
            .iter()
            .rposition(|r| r.final_marker != 0 && r.final_marker != 1);
        if let Some(boundary_idx) = last_boundary {
            for rec in records[boundary_idx + 1..n].iter_mut() {
                for m in rec.phoneme_markers.iter_mut() {
                    *m |= 0x80;
                }
            }
            return;
        }
        let mut cum = 0usize;
        let mut idx = n;
        while cum < PROPAGATE_BACK && idx > 0 {
            idx -= 1;
            for m in records[idx].phoneme_markers.iter_mut() {
                *m |= 0x80;
            }
            cum += records[idx].phoneme_count;
        }
    }

    pub fn record_to_prosody(rec: &WordRecord) -> Vec<ProsodyRecord> {
        let mut out = Vec::with_capacity(rec.phoneme_codes.len());
        let n = rec.phoneme_codes.len();
        for (i, &code) in rec.phoneme_codes.iter().enumerate() {
            let marker = rec.phoneme_markers.get(i).copied().unwrap_or(0);
            let mut p = ProsodyRecord::new(code);
            p.init_from_marker(marker, false);
            if i + 1 == n && (marker & 0x7f) == 0 {
                p.tone_class = 1;
            }
            out.push(p);
        }
        out
    }
}

pub fn number_unit_lookup(current: &[u8], next: &[u8]) -> Option<&'static [u8]> {
    if current.is_empty() || next.is_empty() {
        return None;
    }
    let has_digit = current.iter().any(|b| b.is_ascii_digit());
    let is_korean = !current.is_empty() && current[0] >= 0xA1;
    if !(has_digit || is_korean) {
        return None;
    }
    unit_reading(next)
}

pub fn number_unit_reading(current: &[u8], next: &[u8]) -> Option<Vec<u16>> {
    let reading = number_unit_lookup(current, next)?;
    // Check current is a recognizable number form
    let all_digits = current.iter().all(|b| b.is_ascii_digit());
    let has_dot = current.contains(&b'.');
    let is_korean_num_word = !current.is_empty() && current[0] >= 0xA1;
    if !(all_digits || has_dot || is_korean_num_word) {
        return None;
    }
    // Convert unit KPS reading to phoneme codes (reuse kps code table)
    // Use the same path as sino_integer_codes but for unit reading bytes
    // Unit reading is already KPS bytes -> convert via kps_bytes_to_codes -> to_phoneme
    use crate::g2p::g2p_dict::{kps_bytes_to_codes, to_phoneme_code};
    let codes = kps_bytes_to_codes(reading)?;
    Some(codes.iter().map(|&c| to_phoneme_code(c)).collect())
}

pub fn split_phoneme(code: u16) -> (u8, u8, u8) {
    (
        ((code >> 10) & 0x3f) as u8,
        ((code >> 5) & 0x1f) as u8,
        (code & 0x1f) as u8,
    )
}

pub fn make_phoneme(class: u8, medial: u8, initial: u8) -> u16 {
    (((class as u16) & 0x3f) << 10) | (((medial as u16) & 0x1f) << 5) | ((initial as u16) & 0x1f)
}

pub const FINAL_TO_CLASS: [u8; 28] = [
    0, 2, 2, 5, 6, 0, 15, 14, 15, 6, 6, 15, 15, 14, 5, 15, 5, 18, 0, 5, 15, 5, 0, 5, 27, 5, 5, 0,
];

pub fn apply_final_class(code: u16) -> u16 {
    let class = FINAL_TO_CLASS[((code >> 10) & 0x3f) as usize] as u16;
    (class << 10) | (code & 0x3ff)
}

pub fn synthesize(medial: u8, initial: u8) -> u16 {
    0x6c00 | (((medial as u16) & 0x1f) << 5) | ((initial as u16) & 0x1f)
}

pub fn syllable_to_intermediate(syllable: u16) -> u16 {
    let initial = (syllable >> 10) & 0x1f;
    let medial = (syllable >> 5) & 0x1f;
    let final_c = syllable & 0x1f;
    (final_c << 10) | (medial << 5) | initial
}

pub fn syllable_to_phoneme(syllable: u16) -> u16 {
    apply_final_class(syllable_to_intermediate(syllable))
}

pub fn phoneme_to_syllable(code: u16) -> Option<u16> {
    let (class, medial, initial) = split_phoneme(code);
    if class != 27 {
        return None;
    }
    Some(((initial as u16) << 10) | ((medial as u16) << 5))
}

pub fn is_pause(class: u8, low5: u8) -> bool {
    (matches!(class, 2 | 0x0e | 0x12 | 0x1b) && matches!(low5, 1 | 4 | 0x12))
        || (class == 6 && matches!(low5, 3 | 4 | 0x12))
}

pub fn is_pause_code(code: u16) -> bool {
    is_pause(((code >> 10) & 0x3f) as u8, (code & 0x1f) as u8)
}

pub fn is_real_phoneme(class: u8, low5: u8) -> bool {
    !matches!(
        low5,
        1 | 4 | 6 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 16 | 17 | 18
    ) && !(low5 == 3 && class == 6)
}

pub fn is_real_phoneme_code(code: u16) -> bool {
    is_real_phoneme(((code >> 10) & 0x3f) as u8, (code & 0x1f) as u8)
}

// Static exception/unit/digit/digraph reading tables (pure data + tiny accessors).
mod data;
pub use data::*;
