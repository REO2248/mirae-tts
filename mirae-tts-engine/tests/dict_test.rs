//! 辞書 pkg モジュール (src/dict.rs) の実データ検証。
//!
//! 参照値はすべて T2_dict.md (オリジナル Future.exe の解析) と実データから導出:
//! - Alphabet.pkg の「가」= node 522 / -BASE=64 / TAIL[64..69]=00 07 00 00 00 (§3.4)
//! - User.pkg 全 8 終端ノードの X ∈ 0..8 (§3.3)
//! - colligation.pkg 全 113,910 終端ノードの X < 115,764 (§3.3)
//! - 種別分布 (§4.1): colligation 0x01=80517 / 0x04=19551 / 0x06=7634 / 0x05=6141、
//!   フラグ付き 113909/115764。NonReg 0x11=749、フラグ付き 287/768。User 0x01×7/0x11×2、フラグ付き 8/9

use std::collections::BTreeSet;
use std::path::Path;

use mirae_tts_engine::dict::{
    key_from_syllables, reverse_key, syllable_to_key, Dict, PrefixMatch, TailEntry, KEY_END,
};

const VOICE_DIR: &str = "/home/user/reo_work/mirae2_re/extracted/미래2.0/Voice";

fn load(name: &str) -> Dict {
    Dict::load(Path::new(VOICE_DIR).join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// BFS で到達可能な終端ノード (node, -BASE=TAIL オフセット) を列挙する。
/// 遷移文字は有効キー文字 c ∈ 1..=0x50 (T2 §3.4 の BFS 条件から 'P' を除いたもの)。
fn reachable_terminals(d: &Dict) -> Vec<(usize, usize)> {
    let mut seen = vec![false; d.n1()];
    let mut stack = vec![1usize];
    seen[1] = true;
    let mut terms = Vec::new();
    while let Some(n) = stack.pop() {
        let b = d.base(n).unwrap();
        if b < 0 {
            terms.push((n, (-b) as usize));
            continue;
        }
        for c in 1..=KEY_END {
            let t = b + c as i32;
            if t >= 0 && (t as usize) < d.n1() {
                let t = t as usize;
                if d.check(t) == Some(c) && !seen[t] {
                    seen[t] = true;
                    stack.push(t);
                }
            }
        }
    }
    terms
}

// ---------------------------------------------------------------------------
// 1. パース: 全 5 pkg でヘッダ値一致 + ファイルサイズ完全消費
// ---------------------------------------------------------------------------

#[test]
fn parse_all_pkgs_exact_header_and_full_consumption() {
    // (ファイル, n1, n2, c2, b2, c3, b3, ファイルサイズ) — SPEC §1.3 の表と一致
    let cases: [(&str, usize, usize, usize, usize, usize, usize, usize); 5] = [
        ("Alphabet.pkg", 1915, 5070, 0, 0, 0, 0, 14669),
        ("Conjects.pkg", 799, 2770, 0, 0, 0, 0, 6789),
        ("NonReg.pkg", 54227, 4142, 768, 26, 721, 0, 298863),
        ("User.pkg", 100, 126, 9, 0, 2, 0, 756),
        (
            "colligation.pkg",
            200048,
            814804,
            115764,
            2,
            265,
            0,
            2516558,
        ),
    ];
    for (name, n1, n2, c2, b2, c3, b3, size) in cases {
        let path = Path::new(VOICE_DIR).join(name);
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.len() as usize, size, "{name}: ファイルサイズ");
        // from_bytes は末尾まで完全消費しないと Err を返す (パース成功 = 完全消費)
        let d = Dict::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(d.n1(), n1, "{name}: n1");
        assert_eq!(d.n2(), n2, "{name}: n2");
        assert_eq!(d.sub_a_count(), c2, "{name}: c2");
        assert_eq!(d.sub_a_pairs().len(), b2, "{name}: b2");
        assert_eq!(d.sub_b_count(), c3, "{name}: c3");
        assert_eq!(d.sub_b_pairs().len(), b3, "{name}: b3");
        // 開始ノード 1 (T2 §3.1)。CHECK[1]=1 は全 pkg で成立するが、BASE[1] は
        // pkg により異なる (Alphabet/User は 1、Conjects 742、NonReg 10740、
        // colligation 174749 — 実データで確認済み)。開始ノードは常に 1。
        assert_eq!(d.check(1), Some(1), "{name}: CHECK[1]");
        assert!(d.base(1).is_some_and(|b| b >= 0), "{name}: BASE[1] が負");
    }
}

// ---------------------------------------------------------------------------
// 2. Alphabet.pkg: 「가」= node 522, -BASE=64, TAIL[64] = 00 07 00 00 00 (T2 §3.4)
// ---------------------------------------------------------------------------

#[test]
fn alphabet_ga_reaches_node_522_tail_offset_64() {
    let d = load("Alphabet.pkg");
    // 「가」= 初声1(ㄱ) + 中声1(ㅏ) → u16 0x420 → キー 01 14
    let key = key_from_syllables(&[0x0420]).unwrap();
    assert_eq!(key, vec![0x01, 0x14]);
    // トライ遷移: node 1 → (0x01) → … → (0x14) → … → (0x50) → node 522
    let mut node = 1usize;
    for &c in &[0x01u8, 0x14, KEY_END] {
        let t = d.base(node).unwrap() + c as i32;
        assert!(
            t >= 0 && (t as usize) < d.n1(),
            "遷移 t が範囲外 (node {node})"
        );
        assert_eq!(d.check(t as usize), Some(c), "CHECK 不一致 (node {node})");
        node = t as usize;
    }
    assert_eq!(node, 522, "「가」の終端ノード (T2 §3.4)");
    assert_eq!(
        d.base(522),
        Some(-64),
        "終端ノードの BASE = -TAIL オフセット"
    );
    // 完全一致検索 (FUN_004115c0 相当)
    let off = d.search_exact(&key).unwrap();
    assert_eq!(off, 64);
    // TAIL[64..69] = 00 07 00 00 00 (hexdump 突き合わせ)
    assert_eq!(&d.tail_bytes()[64..69], &[0x00, 0x07, 0x00, 0x00, 0x00]);
    // エントリ: suffix 空 / X=7 / Y=0
    assert!(d.tail_string(64).unwrap().is_empty());
    assert_eq!(d.tail_entry(64), Some(TailEntry { x: 7, y: 0 }));
    assert_eq!(d.tail_entry(64).unwrap().value(), 7);
    // FUN_004119d0 相当
    assert_eq!(d.lookup(&key), Some(TailEntry { x: 7, y: 0 }));
    // Alphabet はサブ構造A を持たない (c2=0) → レコード展開は空
    assert_eq!(d.lookup_records(&key), Some(vec![]));
    // 見つからないキー (「가」+ 余分) は None
    let mut bad = key.clone();
    bad.push(0x01);
    assert_eq!(d.search_exact(&bad), None);
}

// ---------------------------------------------------------------------------
// 3. Alphabet.pkg: 全 1,007 終端ノード (T2 §3.4)、X ∈ 1..7、Y = 0 (§3.3)
// ---------------------------------------------------------------------------

#[test]
fn alphabet_all_1007_terminal_entries_class_codes() {
    let d = load("Alphabet.pkg");
    let terms = reachable_terminals(&d);
    assert_eq!(terms.len(), 1007, "Alphabet 終端ノード数 (T2 §3.4)");
    for &(node, off) in &terms {
        assert!(off < d.n2(), "ノード {node}: TAIL オフセット範囲外");
        let e = d
            .tail_entry(off)
            .unwrap_or_else(|| panic!("ノード {node}: エントリ不正"));
        assert!(
            (1..=7).contains(&e.x),
            "ノード {node}: Alphabet の X は 1..7 (音節クラス)"
        );
        assert_eq!(e.y, 0, "ノード {node}: Alphabet の Y は全て 0");
    }
}

// ---------------------------------------------------------------------------
// 4. User.pkg: 全 8 終端ノードの X ∈ 0..8 (T2 §3.3)、c2=9 と整合
// ---------------------------------------------------------------------------

#[test]
fn user_pkg_all_8_terminal_entries_x_in_0_8() {
    let d = load("User.pkg");
    let terms = reachable_terminals(&d);
    assert_eq!(terms.len(), 8, "User.pkg 終端ノード数 (T2 §3.3)");
    let mut xs = BTreeSet::new();
    for &(node, off) in &terms {
        let e = d
            .tail_entry(off)
            .unwrap_or_else(|| panic!("ノード {node}: エントリ不正"));
        xs.insert(e.x);
        assert!(
            (e.x as usize) < d.sub_a_count(),
            "ノード {node}: X={} が c2={} 以上",
            e.x,
            d.sub_a_count()
        );
    }
    // 実データの X 値集合 (T2: 「全 8 終端ノードの X ∈ 0..8」)
    assert_eq!(xs, [0u16, 1, 2, 3, 4, 5, 6, 8].into_iter().collect());
}

// ---------------------------------------------------------------------------
// 5. colligation.pkg: 全 113,910 終端ノードの X < 115,764 (T2 §3.3)
// ---------------------------------------------------------------------------
// 注: T2 の BFS は遷移文字 c ∈ 0..=0x51 (t2_user_check.py と同一条件)。c=0 の
// 「NUL エッジ」経由で到達する 3 ノード (199879/199905/199918) は -BASE が TAIL
// 範囲外のガベージ値のため、エントリ検証は TAIL 範囲内のものだけを対象とする。

#[test]
fn colligation_113910_terminal_entries_x_within_sub_a() {
    let d = load("colligation.pkg");
    let mut seen = vec![false; d.n1()];
    let mut stack = vec![1usize];
    seen[1] = true;
    let mut terminal = 0usize;
    let mut bad_x = 0usize;
    let mut garbage_offsets = 0usize;
    while let Some(n) = stack.pop() {
        let b = d.base(n).unwrap();
        if b < 0 {
            terminal += 1;
            let off = (-b) as usize;
            match d.tail_entry(off) {
                Some(e) => {
                    if (e.x as usize) >= d.sub_a_count() {
                        bad_x += 1;
                    }
                }
                None => garbage_offsets += 1,
            }
            continue;
        }
        for c in 0..=0x51u8 {
            let t = b + c as i32;
            if t >= 0 && (t as usize) < d.n1() {
                let t = t as usize;
                if d.check(t) == Some(c) && !seen[t] {
                    seen[t] = true;
                    stack.push(t);
                }
            }
        }
    }
    assert_eq!(terminal, 113_910, "colligation 終端ノード数 (T2 §3.3)");
    assert_eq!(bad_x, 0, "X が c2=115764 以上のエントリ");
    assert_eq!(
        garbage_offsets, 3,
        "TAIL 範囲外オフセット (c=0 エッジ経由の 3 ノード)"
    );
}

// ---------------------------------------------------------------------------
// 6. 全 pkg ラウンドトリップ: 格納キーを再構成 → search_exact / search_prefix で復元
// ---------------------------------------------------------------------------

#[test]
fn every_stored_key_round_trips_exact_and_prefix_search() {
    let cases: [(&str, usize); 5] = [
        ("Alphabet.pkg", 1007),
        ("Conjects.pkg", 395),
        ("NonReg.pkg", 261),
        ("User.pkg", 8),
        ("colligation.pkg", 113_907),
    ];
    for (name, expect) in cases {
        let d = load(name);
        // BFS (c ∈ 1..=0x50: 有効キー文字のみ) で全到達可能終端ノードの親パスを記録
        let mut parent: Vec<Option<(usize, u8)>> = vec![None; d.n1()];
        parent[1] = Some((0, 0));
        let mut stack = vec![1usize];
        let mut order = Vec::new();
        while let Some(n) = stack.pop() {
            let b = d.base(n).unwrap();
            if b < 0 {
                order.push(n);
                continue;
            }
            for c in 1..=KEY_END {
                let t = b + c as i32;
                if t >= 0 && (t as usize) < d.n1() {
                    let t = t as usize;
                    if d.check(t) == Some(c) && parent[t].is_none() {
                        parent[t] = Some((n, c));
                        stack.push(t);
                    }
                }
            }
        }
        let mut checked = 0usize;
        for &n in &order {
            let b = d.base(n).unwrap();
            let off = (-b) as usize;
            assert!(off < d.n2(), "{name}: ノード {n} のオフセット範囲外");
            // トライ部パスを復元
            let mut path = Vec::new();
            let mut cur = n;
            while cur != 1 {
                let (p, c) = parent[cur].expect("親なし");
                path.push(c);
                cur = p;
            }
            path.reverse();
            // 格納キー = パス + TAIL サフィックス (常に 'P' 終端)
            let suffix = d
                .tail_string(off)
                .unwrap_or_else(|| panic!("{name}: ノード {n} のサフィックスに NUL なし"));
            let mut full = path.clone();
            full.extend_from_slice(suffix);
            assert_eq!(
                full.last(),
                Some(&KEY_END),
                "{name}: ノード {n} の格納キーは 'P' 終端"
            );
            full.pop(); // 検索キー = 格納キー − 'P'
                        // 完全一致 (FUN_004115c0 相当)
            let got = d.search_exact(&full).unwrap_or_else(|| {
                panic!("{name}: ノード {n} のキー {} が見つからない", hex(&full))
            });
            assert_eq!(got, off, "{name}: ノード {n} のオフセット不一致");
            // プレフィクス検索 (FUN_00411190 相当) でも全キー一致
            let pm = d
                .search_prefix(&full)
                .unwrap_or_else(|| panic!("{name}: ノード {n} の prefix 検索失敗"));
            assert_eq!(pm.tail_offset, off);
            assert_eq!(pm.matched, full.len(), "{name}: ノード {n} のマッチ長");
            checked += 1;
        }
        assert_eq!(checked, expect, "{name}: 終端エントリ数");
    }
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// 7. NonReg: キー反転 + プレフィクス/置換パス (FUN_00411190 相当)
// ---------------------------------------------------------------------------

#[test]
fn nonreg_reversed_keys_round_trip() {
    let d = load("NonReg.pkg");
    // 終端ノードの格納キー (反転済み) を再構成し、reverse_key で元のキーに戻せることを確認
    let mut parent: Vec<Option<(usize, u8)>> = vec![None; d.n1()];
    parent[1] = Some((0, 0));
    let mut stack = vec![1usize];
    let mut terms = Vec::new();
    while let Some(n) = stack.pop() {
        let b = d.base(n).unwrap();
        if b < 0 {
            terms.push(n);
            continue;
        }
        for c in 1..=KEY_END {
            let t = b + c as i32;
            if t >= 0 && (t as usize) < d.n1() {
                let t = t as usize;
                if d.check(t) == Some(c) && parent[t].is_none() {
                    parent[t] = Some((n, c));
                    stack.push(t);
                }
            }
        }
    }
    assert_eq!(terms.len(), 261);
    for &n in &terms {
        let off = (-d.base(n).unwrap()) as usize;
        let mut path = Vec::new();
        let mut cur = n;
        while cur != 1 {
            let (p, c) = parent[cur].unwrap();
            path.push(c);
            cur = p;
        }
        path.reverse();
        let suffix = d.tail_string(off).unwrap();
        let mut stored = path.clone();
        stored.extend_from_slice(suffix);
        assert_eq!(stored.last(), Some(&KEY_END));
        stored.pop();
        // 格納キーは反転済み → 反転すると元の単語キー
        let word_key = reverse_key(&stored);
        // 元キーを反転し直したものが格納キー → 検索でヒット
        let hit = d.search_exact(&reverse_key(&word_key)).unwrap();
        assert_eq!(hit, off);
        // エントリの X は c2=768 未満 (T2 §3.3)
        let e = d.tail_entry(off).unwrap();
        assert!((e.x as usize) < d.sub_a_count(), "X={} >= c2", e.x);
    }
    // reverse_key ヘルパ (FUN_00409f40 相当)
    assert_eq!(reverse_key(&[1, 2, 3]), vec![3, 2, 1]);
    assert_eq!(reverse_key(&reverse_key(&[1, 2, 3])), vec![1, 2, 3]);
    assert!(reverse_key(&[]).is_empty());
}

#[test]
fn prefix_search_substitution_and_strstr_paths() {
    let d = load("User.pkg");
    // node 94 の格納キー (トライが 'P' まで消費、TAIL サフィックス空) — 実データから確定
    let key: Vec<u8> = vec![
        0x0b, 0x14, 0x03, 0x18, 0x08, 0x20, 0x01, 0x1a, 0x29, 0x08, 0x1a, 0x13, 0x24,
    ];
    assert_eq!(d.search_exact(&key), Some(16));
    assert_eq!(
        d.search_prefix(&key),
        Some(PrefixMatch {
            tail_offset: 16,
            matched: key.len()
        })
    );
    // 置換パス: 余分文字で遷移失敗 → 失敗文字を 'P' に置換して終端を試行 (FUN_00411190)
    // → 同じエントリ (off=16) が最長一致、matched = キー長
    for extra in [0x01u8, 0x2a, 0x43, 0x44] {
        let mut k2 = key.clone();
        k2.push(extra);
        assert_eq!(
            d.search_prefix(&k2),
            Some(PrefixMatch {
                tail_offset: 16,
                matched: key.len()
            }),
            "余分文字 {extra:#04x}"
        );
        // 完全一致 API は余分文字で失敗する
        assert_eq!(d.search_exact(&k2), None);
    }
    // node 14 のキー (TAIL サフィックス非空) → 完全一致は strstr パスで full match
    let k14: Vec<u8> = vec![0x0d, 0x1e, 0x08, 0x1a, 0x45, 0x0d, 0x25, 0x07, 0x16, 0x3b];
    assert_eq!(d.search_exact(&k14), Some(111));
    assert_eq!(
        d.search_prefix(&k14),
        Some(PrefixMatch {
            tail_offset: 111,
            matched: k14.len()
        })
    );
    // 余分文字を足すとサフィックス前置一致が崩れて None
    for extra in [0x01u8, 0x2a, 0x43, 0x44] {
        let mut k2 = k14.clone();
        k2.push(extra);
        assert_eq!(d.search_prefix(&k2), None, "余分文字 {extra:#04x}");
    }
}

// ---------------------------------------------------------------------------
// 8. キー変換 (FUN_0040a930 / FUN_0040a470 相当)
// ---------------------------------------------------------------------------

#[test]
fn syllable_key_conversion() {
    // 「가」= 初声1 中声1 → 01 14 (T2 §3.2)
    assert_eq!(key_from_syllables(&[0x0420]).unwrap(), vec![0x01, 0x14]);
    // 「잿」= ㅈ(13) ㅐ(2) ㅅ(19) → 0d 15 3b (T2 §3.2)
    assert_eq!(
        key_from_syllables(&[0x3453]).unwrap(),
        vec![0x0d, 0x15, 0x3b]
    );
    // 初声のみ / 中声のみ / 終声のみ
    assert_eq!(key_from_syllables(&[1 << 10]).unwrap(), vec![0x01]); // ㄱ
    assert_eq!(key_from_syllables(&[1 << 5]).unwrap(), vec![0x14]); // ㅏ
    assert_eq!(key_from_syllables(&[0x0001]).unwrap(), vec![0x29]); // ㄱ終声
    assert_eq!(key_from_syllables(&[0x001b]).unwrap(), vec![0x43]); // ㅎ終声 (27 → 0x43)
                                                                    // 最大値: 初声19 中声21 終声27
    assert_eq!(
        key_from_syllables(&[(19 << 10) | (21 << 5) | 27]).unwrap(),
        vec![0x13, 0x28, 0x43]
    );
    // 特殊文字 (FUN_0040a470): '0'-'9' → 0x46..0x4F、'-' → 'E'、'.' → 'D'
    assert_eq!(key_from_syllables(&[0x8000 | 0x30]).unwrap(), vec![0x46]);
    assert_eq!(key_from_syllables(&[0x8000 | 0x39]).unwrap(), vec![0x4f]);
    assert_eq!(key_from_syllables(&[0x8000 | 0x2d]).unwrap(), vec![0x45]);
    assert_eq!(key_from_syllables(&[0x8000 | 0x2e]).unwrap(), vec![0x44]);
    assert_eq!(key_from_syllables(&[0x8000 | 0x41]), None); // 未定義の特殊文字
                                                            // 変換不能が混ざると全体が None
    assert_eq!(key_from_syllables(&[0x0420, 0x8000 | 0x41]), None);
    // syllable_to_key は出力を追記する
    let mut out = vec![0x01];
    assert!(syllable_to_key(0x0420, &mut out));
    assert_eq!(out, vec![0x01, 0x01, 0x14]);
    // ㅈㅗㄷ (좋) → FUN_0040a930 の写像では 0d 1c 2f (ㅈ=13, ㅗ=9, ㄷ=7)。
    // 注: T2 §3.5 の Conjects 格納キー「13 20 07」はこれとは異なる写像 (未確定) で、
    // conjects_x_are_connect_blob_indices テストで trie 上の事実として検証する。
    assert_eq!(
        key_from_syllables(&[(13 << 10) | (9 << 5) | 7]).unwrap(),
        vec![0x0d, 0x1c, 0x2f]
    );
}

// ---------------------------------------------------------------------------
// 9. サブ構造A レコード展開 (FUN_00411790 相当) と種別分布 (T2 §4.1)
// ---------------------------------------------------------------------------

#[test]
fn sub_a_record_expansion_stops_at_0x80_flag() {
    let d = load("colligation.pkg");
    // X=0 からの展開: 先頭はマスク済み、停止レコード (bit7) を含めて 2 件 (実データで確定)
    let recs = d.expand_records(0);
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[0].kind & 0x80, 0, "先頭レコードは & 0x7F マスク済み");
    assert_eq!(recs[1].kind, 0x81, "停止レコードは生のまま含まれる");
    // 範囲外 index → 空
    assert!(d.expand_records(d.sub_a_count()).is_empty());
    assert!(d.expand_records(usize::MAX).is_empty());
    // 全ての終端エントリの X で展開可能 (空でない)
    let terms = reachable_terminals(&d);
    for &(node, off) in &terms {
        let e = d.tail_entry(off).unwrap();
        let recs = d.expand_records(e.x as usize);
        assert!(!recs.is_empty(), "ノード {node}: X={} で展開不能", e.x);
        // 最後のレコードは bit7 持ち (ラン終端) か、ファイル末尾で打ち切り
        if e.x as usize + recs.len() < d.sub_a_count() {
            assert_ne!(recs.last().unwrap().kind & 0x80, 0, "ノード {node}");
        }
        // 途中レコードは bit7 なし
        for r in &recs[1..recs.len() - 1] {
            assert_eq!(r.kind & 0x80, 0, "ノード {node}");
        }
    }
}

#[test]
fn sub_a_kind_distribution_matches_t2() {
    // colligation (T2 §4.1): 種別 0x01=80517, 0x04=19551, 0x06=7634, 0x05=6141, フラグ付き 113909/115764
    let d = load("colligation.pkg");
    let mut kind_counts = [0usize; 128];
    let mut flagged = 0usize;
    for i in 0..d.sub_a_count() {
        let r = d.sub_a_record(i).unwrap();
        kind_counts[(r.kind & 0x7f) as usize] += 1;
        if r.kind & 0x80 != 0 {
            flagged += 1;
        }
    }
    assert_eq!(kind_counts[0x01], 80_517);
    assert_eq!(kind_counts[0x04], 19_551);
    assert_eq!(kind_counts[0x06], 7_634);
    assert_eq!(kind_counts[0x05], 6_141);
    assert_eq!(flagged, 113_909);
    // NonReg (T2 §4.1): 種別 0x11 = 749/768、フラグ付き 287/768
    let d = load("NonReg.pkg");
    let mut kind_counts = [0usize; 128];
    let mut flagged = 0usize;
    for i in 0..d.sub_a_count() {
        let r = d.sub_a_record(i).unwrap();
        kind_counts[(r.kind & 0x7f) as usize] += 1;
        if r.kind & 0x80 != 0 {
            flagged += 1;
        }
    }
    assert_eq!(kind_counts[0x11], 749);
    assert_eq!(flagged, 287);
    // User (T2 §4.1): 種別 0x01 ×7, 0x11 ×2、フラグ付き 8/9
    let d = load("User.pkg");
    let mut kind_counts = [0usize; 128];
    let mut flagged = 0usize;
    for i in 0..d.sub_a_count() {
        let r = d.sub_a_record(i).unwrap();
        kind_counts[(r.kind & 0x7f) as usize] += 1;
        if r.kind & 0x80 != 0 {
            flagged += 1;
        }
    }
    assert_eq!(kind_counts[0x01], 7);
    assert_eq!(kind_counts[0x11], 2);
    assert_eq!(flagged, 8);
}

#[test]
fn lookup_records_expands_user_entries() {
    let d = load("User.pkg");
    // node 94 のキー → X=0 → 展開される
    let key: Vec<u8> = vec![
        0x0b, 0x14, 0x03, 0x18, 0x08, 0x20, 0x01, 0x1a, 0x29, 0x08, 0x1a, 0x13, 0x24,
    ];
    let recs = d.lookup_records(&key).unwrap();
    assert!(!recs.is_empty());
    assert_eq!(recs[0].kind & 0x80, 0, "先頭レコードはマスク済み");
    // 全 8 終端キーで lookup_records (FUN_00411990 相当) が成功する
    let mut parent: Vec<Option<(usize, u8)>> = vec![None; d.n1()];
    parent[1] = Some((0, 0));
    let mut stack = vec![1usize];
    while let Some(n) = stack.pop() {
        let b = d.base(n).unwrap();
        if b < 0 {
            continue;
        }
        for c in 1..=KEY_END {
            let t = b + c as i32;
            if t >= 0 && (t as usize) < d.n1() {
                let t = t as usize;
                if d.check(t) == Some(c) && parent[t].is_none() {
                    parent[t] = Some((n, c));
                    stack.push(t);
                }
            }
        }
    }
    let mut found = 0usize;
    for n in 0..d.n1() {
        if d.base(n).is_some_and(|b| b < 0) {
            let mut path = Vec::new();
            let mut cur = n;
            while cur != 1 {
                let (p, c) = parent[cur].expect("終端ノードが未到達");
                path.push(c);
                cur = p;
            }
            path.reverse();
            let off = (-d.base(n).unwrap()) as usize;
            let suffix = d.tail_string(off).unwrap();
            let mut full = path;
            full.extend_from_slice(suffix);
            assert_eq!(full.pop(), Some(KEY_END));
            let recs = d.lookup_records(&full).expect("lookup_records 失敗");
            assert!(!recs.is_empty());
            found += 1;
        }
    }
    assert_eq!(found, 8);
}

// ---------------------------------------------------------------------------
// 10. エラー処理・境界
// ---------------------------------------------------------------------------

#[test]
fn malformed_input_rejected_and_search_bounds() {
    // 切り詰めデータはエラー
    let good = std::fs::read(Path::new(VOICE_DIR).join("User.pkg")).unwrap();
    for cut in [0usize, 1, 7, 8, 100, good.len() - 1] {
        assert!(Dict::from_bytes(&good[..cut]).is_err(), "cut={cut}");
    }
    // 末尾に余分バイト → サイズ不一致エラー
    let mut padded = good.clone();
    padded.push(0);
    assert!(Dict::from_bytes(&padded).is_err());
    // 空キーは None (FUN_004115c0 は strlen==0 で -1)
    let d = load("Alphabet.pkg");
    assert_eq!(d.search_exact(&[]), None);
    assert_eq!(d.search_prefix(&[]), None);
    assert_eq!(d.lookup(&[]), None);
    // ありえない長キーは安全に None (無限ループしない)
    let junk = vec![0x01u8; 64];
    assert_eq!(d.search_exact(&junk), None);
    assert_eq!(d.search_prefix(&junk), None);
    // 範囲外アクセサ
    assert_eq!(d.base(usize::MAX), None);
    assert_eq!(d.check(usize::MAX), None);
    assert_eq!(d.tail(usize::MAX), None);
    assert_eq!(d.tail_string(d.n2()), None);
    assert_eq!(d.tail_entry(d.n2()), None);
    assert_eq!(d.sub_a_record(d.sub_a_count()), None);
}

#[test]
fn conjects_x_are_connect_blob_indices() {
    // Conjects.pkg の X は Connect.pkg のブロブ index (T2 §3.3: 401 ブロブ)
    let d = load("Conjects.pkg");
    let terms = reachable_terminals(&d);
    assert_eq!(terms.len(), 395);
    for &(node, off) in &terms {
        let e = d
            .tail_entry(off)
            .unwrap_or_else(|| panic!("ノード {node}: エントリ不正"));
        assert!(
            (e.x as u32) < 401,
            "ノード {node}: X={} が Connect ブロブ数 401 以上",
            e.x
        );
    }
    // T2 §3.5 の参照値: 「n 12: key = 13 20 07 | 16 46 50 (좋 + サフィックス)」。
    // トライ部 13 20 07 は node 12 (終端, -BASE=103)、TAIL サフィックス 16 46 50。
    let mut node = 1usize;
    for &c in &[0x13u8, 0x20, 0x07] {
        let t = d.base(node).unwrap() + c as i32;
        assert!(t >= 0 && (t as usize) < d.n1(), "遷移範囲外 (node {node})");
        assert_eq!(d.check(t as usize), Some(c), "CHECK 不一致 (node {node})");
        node = t as usize;
    }
    assert_eq!(node, 12, "T2 §3.5: トライ部の終端ノード");
    assert_eq!(d.base(12), Some(-103), "T2 §3.5: -BASE = TAIL オフセット");
    // 完全キー ('P' 除く) = 13 20 07 16 46 → オフセット 103、サフィックス 16 46 50
    let off = d
        .search_exact(&[0x13, 0x20, 0x07, 0x16, 0x46])
        .expect("T2 §3.5 のキーが見つからない");
    assert_eq!(off, 103);
    assert_eq!(d.tail_string(103).unwrap(), &[0x16, 0x46, 0x50]);
    let e = d.tail_entry(103).unwrap();
    assert!(
        (e.x as u32) < 401,
        "X={} は Connect ブロブ index (401 未満)",
        e.x
    );
}
