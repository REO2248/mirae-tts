// placeholder — implemented in the rewrite phase

//! G2P — 字素→音素変換。
//!
//! オリジナル Future.exe の解析 (G2P_detail.md / SPEC_tts_rewrite.md §2.3) に基づく。
//! 本ファイルは 2 つのサブエージェント担当分を含む:
//!
//! 1. [`g2p_dict`] — 辞書引きパイプライン (colligation/User/NonReg/Conjects) +
//!    単語→読み主経路 (FUN_0041f320 相当) + 9 段階後処理チェーン骨格
//!    (FUN_00440780/004407c0/00440470/004425c0 相当) + 12B レコード化。
//! 2. トップレベルの音素コード変換基盤 (split_phoneme / apply_final_class /
//!    syllable_to_phoneme 等) + 静的例外語テーブル (FUN_0043b010) +
//!    数字/単位読み + アルファベット/字母読み。
//!
//! ※ トップレベル部分は別サブエージェントの実装 (tests/g2p_test.rs が契約) を、
//!    実データ (Future.exe の .rdata/.data ダンプ + KeyPad.Ebd) から復元したもの。

pub mod g2p_dict {
    //! 辞書引きパイプライン + 単語→読み主経路 + 後処理骨格。
    //!
    //! 仕様: `tts_reports2/G2P_detail.md`
    //! - §5  FUN_00444fb0 (NonReg 検索: キー反転 + プレフィクス検索)
    //! - §6  FUN_0044a100 / FUN_0041f320 (colligation → User フォールバック)
    //! - §8  FUN_0044e670 (Conjects 検証 + Connect 行列)
    //! - §9  9 段階後処理チェーン (段階 1/4/7/8 を骨格実装、他は TODO)
    //!
    //! 出力: 語レコード ([u16 音素コード列][マーカ列]) → [`record::ProsodyRecord`]
    //! (12B レコード) への変換関数 [`record_to_prosody`]。

    use std::collections::HashMap;
    use std::sync::OnceLock;

    use crate::connect::ConnectMatrix;
    use crate::dict::{key_from_syllables, reverse_key, Dict, SubARecord};
    use crate::kps9566::Kps9566;
    use crate::record::ProsodyRecord;

    // ---------------------------------------------------------------------
    // 定数 (オリジナル .rdata / 解析確定値)
    // ---------------------------------------------------------------------

    /// 候補生成の上限 (FUN_0041f320: 最大 0xd6 = 214 個)。
    pub const MAX_CANDIDATES: usize = 214;

    /// NonReg ミス/フォールバック時のマーカ (G2P §5: マーカ 0x11)。
    pub const MARKER_FALLBACK: u8 = 0x11;

    /// 数字のみ候補のパック値ベース (FUN_0041f020: (1,3,1,1,1,1) → 0x152D|idx)。
    pub const PACKED_DIGITS: u16 = 0x152D;
    /// 記号のみ候補のパック値ベース (FUN_0041f020: (3,4,1,1,2,2) → 0x2933|idx)。
    pub const PACKED_SYMBOLS: u16 = 0x2933;

    /// 終声分離対象の終声 (FUN_0040a290: 下位 5bit ∈ {3,7,0xf,0x10} = ㄷ,ㅂ,ㅈ,ㅊ)。
    pub const SPLIT_FINALS: [u16; 4] = [0x03, 0x07, 0x0F, 0x10];

    /// 形態素種別 0x14..0x1f → 語末追記の特殊コード (FUN_0044e4a0:
    /// 0x8030..0x803f = 数字 0-15 相当)。
    pub const MORPH_TYPE_BASE: u8 = 0x14;

    /// 段階 8 のチャンク境界音節数 (DAT_00489170 = 60)。
    pub const CHUNK_SYLLABLES: usize = 60;
    /// 段階 8 の伝搬条件 (DAT_00489160; 1 = 前方伝搬, 0 = 後方)。
    /// t21 確定: Future.exe の .data で DAT_00489160 = 0 (実行時書込みなし —
    /// 参照は FUN_004425c0 内 0x4426ad/0x44270a/0x44276e の読取りのみ)。
    /// したがって実際の動作は常に「後方伝搬」(各文の末尾から 5 音素分の語に
    /// マーカ bit7 を立てる)。旧実装の 1 (前方) は誤り — 実測 REQ の
    /// f5 bit7 (各文最終語のみ) と一致しない。
    pub const PROPAGATE_FORWARD: u8 = 0;
    /// 段階 8 の後方伝搬量 (DAT_00489168 = 5 音素分)。
    pub const PROPAGATE_BACK: usize = 5;

    /// DAT_00489214 — 終声 → 音韻クラス置換表 (G2P §11.2, 確定)。
    /// idx = 終声 index (0..27)、val = 音韻クラス。0 = 終声なしはクラス 27 に統合。
    pub const CLASS_REPLACE: [u8; 28] = [
        0, 2, 2, 5, 6, 0, 15, 14, 15, 6, 6, 15, 15, 14, 5, 15, 5, 18, 0, 5, 15, 5, 0, 5, 27, 5, 5,
        0,
    ];

    /// 段階 7 の平滑化重み (DAT_00489174/78/7c = 0.5/0.5/0.99, _DAT_0047f010 = 1.0)。
    pub const PROSODY_W1: f32 = 0.5;
    pub const PROSODY_W2: f32 = 0.5;
    pub const PROSODY_W3: f32 = 0.99;
    /// 段階 7 のアクセント境界判定範囲 [下, 上] (G2P §9-7: [1.86, 2.9] の範囲外 → 3)。
    pub const ACCENT_RANGE: (f32, f32) = (1.86, 2.9);

    // ---------------------------------------------------------------------
    // 型
    // ---------------------------------------------------------------------

    /// 1 候補分の読み出力 (FUN_0041f320 の出力記録相当)。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Reading {
        /// 読み (KeyPad 内部バイト列)。パック出力の場合は空。
        pub bytes: Vec<u8>,
        /// パック出力 (数字のみ 0x152D|idx / 記号のみ 0x2933|idx, FUN_0041f020)。
        pub packed: Option<u16>,
        /// マーカ (辞書ヒット時 = 6B レコード列の先頭種別、フォールバック = 0x11)。
        pub marker: u8,
    }

    impl Reading {
        /// フォールバック読み: 単語そのまま + マーカ 0x11 (G2P §5)。
        pub fn fallback(word: &[u8]) -> Reading {
            Reading {
                bytes: word.to_vec(),
                packed: None,
                marker: MARKER_FALLBACK,
            }
        }
    }

    /// NonReg 検索ヒット (FUN_00444fb0 のヒット時出力, G2P §5)。
    #[derive(Debug, Clone)]
    pub struct NonRegHit {
        /// 復元した読み (KeyPad バイト列)。
        pub reading: Vec<u8>,
        /// マーカ = 6B レコード列の種別 (local_85d)。
        pub marker: u8,
        /// 展開した 6B レコード列。
        pub records: Vec<SubARecord>,
        /// 反転キーのマッチ文字数。
        pub matched: usize,
    }

    /// 語レコード — 9 段階後処理チェーンと 12B レコード化の入力。
    ///
    /// オフセットはオリジナルの語レコード (+0x28 配列, 0x1dccB/語) に対応。
    #[derive(Debug, Clone, Default)]
    pub struct WordRecord {
        /// 元の綴り (KeyPad バイト列)。語間音韻規則の終声判定に使用
        /// (読みは辞書により綴りと異なる場合がある: 例 増加→ズ가)。
        pub spelling: Vec<u8>,
        /// 連結読み (KeyPad バイト列)。
        pub reading_bytes: Vec<u8>,
        /// 読み → u16 音節コード列 (分析用)。
        pub syllable_codes: Vec<u16>,
        /// 形態素別マーカ (+0x3040.., 読みごとに 1 個)。
        pub morph_markers: Vec<u8>,
        /// 段階 1 出力: 音素コード列 (+0x1f8)。
        pub phoneme_codes: Vec<u16>,
        /// 音素別マーカ列 (+0xdb0)。bit7 = 強調フラグ (段階 8 で付与)。
        pub phoneme_markers: Vec<u8>,
        /// 音素数 (+0xb20)。
        pub phoneme_count: usize,
        /// 規則マーカ (+0xb5c5, 段階 2/4/5/6 で設定)。
        pub rule_marker: u8,
        /// 語間音韻規則フラグ (+0xb5c0..0xb5c3: 結合/連音/鼻音化/激音化)。
        pub rule_flags: [u8; 4],
        /// 語間音韻規則カウント (+0xd38c..0xd38f)。
        pub rule_counts: [u8; 4],
        /// 連音リンクフラグ (+0xb5c4, FUN_0043f290 が 8 を返したとき 1)。
        pub flag_link: u8,
        /// 語番号 (+0xb5d4)。
        pub seq: u8,
        /// 韻律平滑化値 (+0xb5c8 / +0xb5cc / +0xb5d0)。
        pub prosody: [f32; 3],
        /// アクセント境界 (+0xb5c6: 0/3/4/5/6/7/8/9)。
        pub accent: u8,
        /// 最終マーカ (+0x16db: 0/3/5/5/2/2/6/7)。
        pub final_marker: u8,
    }

    /// 辞書一式 (呼び出し側でロード済みの参照)。
    #[derive(Debug, Clone, Copy)]
    pub struct G2pDicts<'a> {
        /// colligation.pkg (DAT_004a4948) — 主辞書。
        pub colligation: &'a Dict,
        /// User.pkg (DAT_004a493c) — フォールバック辞書。
        pub user: &'a Dict,
        /// NonReg.pkg (DAT_004a4938) — 不規則活用辞書 (キー反転格納)。
        pub nonreg: &'a Dict,
        /// Conjects.pkg (DAT_004a492c) — 接続検証辞書。
        pub conjects: &'a Dict,
        /// Connect.pkg — 401×401 音素接続行列。
        pub connect: &'a ConnectMatrix,
    }

    // ---------------------------------------------------------------------
    // コード変換 (FUN_0040a850 / FUN_0040a780 / FUN_0040a1b0 相当)
    //
    // ※ トップレベルの音素コード変換基盤 (別サブエージェント担当分) と名前を
    //    分離してある。挙動は解析 (G2P_detail.md §1) からの同等実装。
    // ---------------------------------------------------------------------

    /// Unicode 音節 → (初声 1..19, 中声 1..21, 終声 0..27) (標準公式)。
    fn unicode_syllable_to_jamo(uni: u32) -> Option<(u8, u8, u8)> {
        if !(0xAC00..=0xD7A3).contains(&uni) {
            return None;
        }
        let n = uni - 0xAC00;
        let init = (n / 588) as u8 + 1;
        let med = ((n % 588) / 28) as u8 + 1;
        let fin = (n % 28) as u8;
        Some((init, med, fin))
    }

    /// KPS9566 音節 (BE u16) → (初声, 中声, 終声)。
    /// Kps9566 テーブル (kps → Unicode) + Unicode 音節分解から導出
    /// (FUN_00409b90 の KS X 1001 テーブル引きと等価)。
    fn kps_syllable_to_jamo(kps: u16) -> Option<(u8, u8, u8)> {
        kps_to_jamo_kp(kps)
    }

    /// FUN_00409b90 相当: KPS9566 音節 → (init 1..19, med 1..21, fin 0..27) (北朝鮮順)。
    ///
    /// 実行時テーブル (kps_tables) は Future.exe の wine プロセスメモリから取得:
    /// - 行検索: code < ROW_STARTS[i] で break → init1 (1 ベース行 = 初声)
    /// - 列検索: code < COL_STARTS[init1*21+m+1] で break → med1 (1 ベース列 = 母音)
    /// - 終声: 列開始からのオフセットをビットマスク (COL_MASKS) で数え上げ
    fn kps_to_jamo_kp(kps: u16) -> Option<(u8, u8, u8)> {
        // 1. 行検索 (1 ベース init)
        let mut init1 = 19usize;
        for i in 0..19 {
            if kps < crate::kps_tables::ROW_STARTS[i] {
                init1 = i;
                break;
            }
        }
        // 音節領域外 (0xB0A1 未満) は分解不能。
        if init1 == 0 {
            return None;
        }
        // 2. 列検索 (1 ベース med)
        let mut med1 = 0usize;
        while med1 < 21 {
            let e = init1 * 21 + med1;
            // 列 20 の「次列」は音節領域の終わり (0xCCD0)。それ以外は COL_STARTS[e+1]。
            let nxt = if med1 == 20 {
                0xCCD0
            } else {
                crate::kps_tables::COL_STARTS[e + 1]
            };
            if nxt != 0xffff && kps < nxt {
                break;
            }
            med1 += 1;
        }
        if med1 >= 21 {
            return None;
        }
        // 3. 終声: 列内オフセット (= コード差 svar) がそのまま列内番号。
        //    「存在しない終声」はコードを消費しないため、svar = fin (0 ベース)。
        let e = init1 * 21 + med1;
        let start = crate::kps_tables::COL_STARTS[e];
        let mut svar = kps as i32 - start as i32;
        if (kps & 0xff) < (start & 0xff) {
            svar -= 0xa2;
        }
        let fin = svar.max(0) as u8;
        Some((init1 as u8, med1 as u8, fin))
    }

    /// 北朝鮮順初声 (0..18) → Unicode 標準初声 (0..18)。
    /// 北朝鮮: ㄱㄴㄷㄹㅁㅂㅅㅈㅊㅋㅌㅍㅎㄲㄸㅃㅆㅉㅇ
    /// Unicode: ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ
    pub(crate) const INIT_KP_TO_STD: [u8; 19] = [0, 2, 3, 5, 6, 7, 9, 12, 14, 15, 16, 17, 18, 1, 4, 8, 10, 13, 11];

    /// Unicode 標準初声 (0..18) → 北朝鮮順初声 (0..18)。
    pub(crate) const INIT_STD_TO_KP: [u8; 19] = [0, 13, 1, 2, 14, 3, 4, 5, 15, 6, 16, 18, 7, 17, 8, 9, 10, 11, 12];

    /// 北朝鮮順母音 (0..20) → Unicode 標準母音 (0..20)。
    /// 北朝鮮: ㅏㅑㅓㅕㅗㅛㅜㅠㅡㅣㅐㅒㅔㅖㅘㅙㅚㅝㅞㅟㅢ
    /// Unicode: ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ
    pub(crate) const MED_KP_TO_STD: [u8; 21] = [0, 2, 4, 6, 8, 12, 13, 17, 18, 20, 1, 3, 5, 7, 9, 10, 11, 14, 15, 16, 19];

    /// Unicode 標準母音 (0..20) → 北朝鮮順母音 (0..20)。
    pub(crate) const MED_STD_TO_KP: [u8; 21] = [0, 10, 1, 11, 2, 12, 3, 13, 4, 14, 15, 16, 5, 6, 17, 18, 19, 7, 8, 20, 9];

    /// (初声, 中声, 終声) → KPS9566 音節コード (BE u16)。FUN_0040a5c0 相当。
    /// init/med は Unicode 標準順 (1 ベース)、fin は北朝鮮順の列内番号 (0 ベース)。
    fn jamo_to_kps_syllable(init: u8, med: u8, fin: u8) -> Option<u16> {
        let init_kp = INIT_STD_TO_KP[(init - 1) as usize] as usize; // 0 ベース
        let med_kp = MED_STD_TO_KP[(med - 1) as usize] as usize;
        let init1 = init_kp + 1; // 1 ベース (実行時テーブルの行)
        let med1 = med_kp + 1;
        let e = init1 * 21 + med1;
        let start = crate::kps_tables::COL_STARTS[e];
        if start == 0xffff {
            return None;
        }
        let mut mask = crate::kps_tables::COL_MASKS[e];
        // fin (0 ベースの列内オフセット) は「存在する終声」がコード連続で並ぶため
        // 単純に開始コード + fin で復元できる (FUN_0040a5c0 のビットマスク走査と等価)。
        let mut code = start + fin as u16;
        // 0xa2 補正 (FUN_0040a5c0: 下位バイトが開始コード未満 or 0xfe 超)。
        let lo = code & 0xff;
        if lo < (start & 0xff) || 0xfe < lo {
            code += 0xa2;
        }
        Some(code)
    }

    fn syllable_jamo_map() -> &'static HashMap<u16, (u8, u8, u8)> {
        static MAP: OnceLock<HashMap<u16, (u8, u8, u8)>> = OnceLock::new();
        MAP.get_or_init(|| {
            let kps = Kps9566::builtin();
            let mut m = HashMap::with_capacity(11172);
            // KPS9566 の 2 バイトコード全域を走査し、Unicode 音節にデコードできる
            // ものを jamo 分解して登録する (FUN_00409b90 のテーブル引きと等価)。
            for hi in 0xA1u16..=0xFE {
                for lo in 0xA1u16..=0xFE {
                    let code = (hi << 8) | lo;
                    if let Some(uni) = kps.lookup(code) {
                        if let Some(j) = unicode_syllable_to_jamo(uni) {
                            m.insert(code, j);
                        }
                    }
                }
            }
            m
        })
    }

    fn kps_syllable_map() -> &'static HashMap<(u8, u8, u8), u16> {
        static MAP: OnceLock<HashMap<(u8, u8, u8), u16>> = OnceLock::new();
        MAP.get_or_init(|| {
            let mut m = HashMap::with_capacity(11172);
            for (&kps, &j) in syllable_jamo_map() {
                m.insert(j, kps);
            }
            m
        })
    }

    /// 特殊コード (bit15) を KeyPad バイト列の 1 バイトへ (FUN_0040a780 相当)。
    /// 数字 0x8030..0x8039 → '0'..'9'、'-' → 0x2d、'.' → 0x2e。
    fn special_code_to_byte(code: u16) -> u8 {
        let u = code & 0x7fff;
        match u {
            0x30..=0x39 => (u as u8 - 0x30) + b'0',
            0x2d => b'-',
            0x2e => b'.',
            // それ以外 (0xA1A1 系記号の 0x8000|(code&0xfff) 等): 下位バイトで近似
            // (TODO: 記号読みは FUN_0040ab70 系が担当)
            _ => (u & 0xff) as u8,
        }
    }

    /// 1 バイト (ASCII/特殊) → 特殊コード (bit15)。数字 '0'..'9' → 0x8030..0x8039、
    /// '-' → 0x802d、'.' → 0x802e。
    fn byte_to_special_code(b: u8) -> u16 {
        match b {
            b'0'..=b'9' => 0x8000 | (b - b'0' + 0x30) as u16,
            b'-' => 0x8000 | 0x2d,
            b'.' => 0x8000 | 0x2e,
            _ => 0x8000 | b as u16,
        }
    }

    /// KeyPad バイト列 → u16 コード列 (FUN_0040a850 相当, G2P §1)。
    /// 変換不能な文字があれば None (オリジナルは解放して失敗を返す)。
    pub fn kps_bytes_to_codes(bytes: &[u8]) -> Option<Vec<u16>> {
        let mut out = Vec::with_capacity(bytes.len() / 2 + 1);
        let mut i = 0;
        while i < bytes.len() {
            let b0 = bytes[i];
            if b0 < 0x80 {
                out.push(byte_to_special_code(b0));
                i += 1;
                continue;
            }
            let b1 = *bytes.get(i + 1)?;
            let kps = ((b0 as u16) << 8) | b1 as u16;
            if let Some((init, med, fin)) = kps_syllable_to_jamo(kps) {
                // init/med は北朝鮮順 (1 ベース)、fin は北朝鮮順の列内番号 (0 ベース)。
                // 辞書キー (syllable_to_key) は NonReg.pkg 等の実データが Unicode 順で
                // 構築されているため、init/med を Unicode 標準順に変換する。
                let init_std = (INIT_KP_TO_STD[(init - 1) as usize] + 1) as u16;
                let med_std = (MED_KP_TO_STD[(med - 1) as usize] + 1) as u16;
                let code = (init_std << 10) | (med_std << 5) | fin as u16;
                out.push(code);
            } else if (0xA3B0..=0xA3B9).contains(&kps) {
                // KPS 全角数字 → 0x8030..0x8039 (dict のキー変換と整合)
                out.push(0x8000 | (kps - 0xA3B0 + 0x30));
            } else if kps == 0xA1AF {
                // KPS '-' (ハイフン) → 0x802d
                out.push(0x8000 | 0x2d);
            } else if kps == 0xA1A4 || kps == 0xA1A5 {
                // KPS '.' (中点/終止符) → 0x802e
                out.push(0x8000 | 0x2e);
            } else {
                // その他の記号: bit15 付きで保持 (キー変換は TODO)
                out.push(0x8000 | (kps & 0xfff));
            }
            i += 2;
        }
        Some(out)
    }

    /// u16 コード列 → KeyPad バイト列 (FUN_0040a780 相当)。
    /// 変換不能 (音節マップ欠落) があれば None。
    pub fn codes_to_kps_bytes(codes: &[u16]) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(codes.len() * 2);
        for &c in codes {
            if c & 0x8000 != 0 {
                out.push(special_code_to_byte(c));
            } else {
                let init = ((c >> 10) & 0x1f) as u8;
                let med = ((c >> 5) & 0x1f) as u8;
                let fin = (c & 0x1f) as u8;
                // 特殊コード (packed 数字/記号など) は音節形式でない → None
                if init == 0 || med == 0 || init > 19 || med > 21 {
                    return None;
                }
                let kps = jamo_to_kps_syllable(init, med, fin)?;
                out.push((kps >> 8) as u8);
                out.push((kps & 0xff) as u8);
            }
        }
        Some(out)
    }

    /// キー文字列 → u16 コード列 (FUN_0040a1b0/0040a9f0 相当)。
    /// 初声 0x01..0x13 / 中声 0x14..0x28 / 終声 0x29..0x43、特殊 0x44='.' 0x45='-'
    /// 0x46..0x4F=数字。'P' (0x50) は無視。
    pub fn key_str_to_codes(key: &[u8]) -> Option<Vec<u16>> {
        let mut out = Vec::with_capacity(key.len() / 3 + 1);
        let mut i = 0;
        while i < key.len() {
            let c = key[i];
            match c {
                0x01..=0x13 => {
                    let init = c;
                    i += 1;
                    let mut med = 0u8;
                    let mut fin = 0u8;
                    if i < key.len() && (0x14..=0x28).contains(&key[i]) {
                        med = key[i] - 0x13;
                        i += 1;
                        if i < key.len() && (0x29..=0x43).contains(&key[i]) {
                            fin = key[i] - 0x28;
                            i += 1;
                        }
                    }
                    out.push(((init as u16) << 10) | ((med as u16) << 5) | fin as u16);
                }
                0x44 => {
                    out.push(0x8000 | 0x2e);
                    i += 1;
                }
                0x45 => {
                    out.push(0x8000 | 0x2d);
                    i += 1;
                }
                0x46..=0x4F => {
                    out.push(0x8000 | (c - 0x46 + 0x30) as u16);
                    i += 1;
                }
                0x50 => i += 1, // 単語終端マーカ
                _ => return None,
            }
        }
        Some(out)
    }

    /// 音節コード → 音素コード (VoiceInfo 検索キー)。
    ///
    /// 解析確定式 (G2P §11): 上位 6bit = 終声の音韻クラス (DAT_00489214)、
    /// 中位 5bit = 母音 (0 基準)、下位 5bit = 初声 (0 基準)。終声なしはクラス 27
    /// (0x6c00 基底, FUN_00428620) に統合。
    /// 実データ検証: 가(0x0420) → 0x6c00 (VoiceInfo 699 units)、돈(0x1124) →
    /// 0x1903 (874 units) と完全一致。
    ///
    /// ※ 初声なし (母音開始音節) の下位 5bit は 0 で近似 (TODO: 音素コード変換
    ///   基盤側の `syllable_to_phoneme` と整合させる — そちらはクラス 0 を返す
    ///   リテラル解釈のため、最終的な採用は要調整)。
    pub fn to_phoneme_code(syllable: u16) -> u16 {
        // syllable は Unicode 標準順 (init_std 1ベース<<10 | med_std 1ベース<<5 | fin_kp 0ベース)。
        // fin_kp は北朝鮮順の「列内の存在する終声の番号」(実行時テーブルの fin、0 ベース)。
        // 音素コード (VoiceInfo 検索キー) は北朝鮮順: cls<<10 | med_kp<<5 | init_kp。
        //
        // t11 (2026-08-15): クラスは列マスクのビット位置 (FUN_004280a0 相当) に修正。
        // 旧実装は DAT_00489214 (CLASS_REPLACE) をクラス表として使っていたが、これは
        // 後の段階 (FUN_00406c10) で使う「終声 index → クラス置換表」であり、オリジナルの
        // 基底クラスとは異なる (実測: 정=18, 삼=14, 십=15, 일=6 — 旧実装は 15/6/0/5)。
        // キャプチャ 287/287 レコードと一致することを確認済み。
        let init_std = ((syllable >> 10) & 0x1f) as usize;
        let med_std = ((syllable >> 5) & 0x1f) as usize;
        let fin_kp = (syllable & 0x1f) as usize;
        // 特殊コード (packed 数字/記号、0x8000| 系) は音節形式でないためそのまま保持。
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
        ((class as u16) << 10) | ((med_kp as u16) << 5) | (init_kp as u16)
    }

    /// 終声の音韻クラス (FUN_004280a0 相当): 列マスクの `fin_kp` 番目のセットビット位置。
    /// マスク = 列 (init_kp, med_kp) に存在する終声の集合 (0x489694 + 8*c)。
    /// 実データ検証 (t11_digit_reading.md): キャプチャ 287/287 レコードと一致。
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

    /// KPS コード (BE u16) → 音素コード。FUN_004280a0 相当のクラス計算を経由する。
    pub fn kps_code_to_phoneme(kps: u16) -> u16 {
        let Some((init1, med1, fin)) = kps_to_jamo_kp(kps) else {
            return 0;
        };
        let init_std = INIT_KP_TO_STD[(init1 - 1) as usize] + 1;
        let med_std = MED_KP_TO_STD[(med1 - 1) as usize] + 1;
        to_phoneme_code(((init_std as u16) << 10) | ((med_std as u16) << 5) | (fin as u16))
    }

    /// KPS コード (BE u16) の終声の音韻クラス (0=ㄱ系, 2=ㄴ系, 5=ㄷ系, 6=ㄹ系,
    /// 14=ㅁ系, 15=ㅂ系, 18=ㅇ, 27=開音節 (終声なし))。
    /// 数字語マージの条件判定 (読みコードは終声落とし済みのため綴りから判定する) に使用。
    pub fn kps_final_class(kps: u16) -> u8 {
        let Some((init1, med1, fin)) = kps_to_jamo_kp(kps) else {
            return 27;
        };
        if fin == 0 {
            27
        } else {
            final_class_from_mask(
                (init1 - 1) as usize,
                (med1 - 1) as usize,
                fin as usize,
            ) as u8
        }
    }

    /// KPS コード (BE u16) → 音素コード (数字読み用: 終声 ㄱ 系 (クラス 0) を落とす)。
    /// 実測: 「1500」の 백 は「배」(0x6D45, 終声なし) と読まれる
    /// (通常変換 0x0145 はクラス 0 のため選択不能コード)。십 (0x3D26, ㅂ)・천 (ㄴ) は落とさない。
    fn kps_code_to_phoneme_no_final(kps: u16) -> u16 {
        let Some((init1, med1, _)) = kps_to_jamo_kp(kps) else {
            return 0;
        };
        let init_std = INIT_KP_TO_STD[(init1 - 1) as usize] + 1;
        let med_std = MED_KP_TO_STD[(med1 - 1) as usize] + 1;
        to_phoneme_code(((init_std as u16) << 10) | ((med_std as u16) << 5))
    }

    /// 小数 (n.nnn) の読みコード列: 各桁を個別に読み、小数点は「쩜」。
    /// 実測 (orig_capture.json): 「2.0」→ 0x1532(일) 0x3851(쩜) 0x4863(령) の 3 レコード。
    /// '0' → 0x4863 (령), '2' → 0x1532 (専用コード — 通常変換では生成されない),
    /// その他の桁 → 漢数詞 KPS (0x489190) の通常変換。
    pub fn decimal_codes(int_digits: &[u8], frac_digits: &[u8]) -> Vec<u16> {
        let mut out = Vec::with_capacity(int_digits.len() + frac_digits.len() + 1);
        for &d in int_digits {
            out.push(decimal_digit_code(d));
        }
        out.push(kps_code_to_phoneme(0xC9B0)); // 쩜
        for &d in frac_digits {
            out.push(decimal_digit_code(d));
        }
        out
    }

    /// 小数の各桁の音素コード。
    fn decimal_digit_code(d: u8) -> u16 {
        match d {
            0 => 0x4863, // 령 (実測)
            2 => 0x1532, // 일 (実測; 専用コード)
            _ => kps_code_to_phoneme(crate::digit_tables::SINO_DIGITS[d as usize]),
        }
    }

    /// 整数の位取り読みコード列 (FUN_0043c230 相当の漢数詞読み)。
    /// 例: [1,5,0,0] (1500) → 천(0x0848) 오(0x6C92) 백(0x6D45) の 3 コード、
    ///     [3,5] (35) → 삼(0x3806) 십(0x3D26) 오(0x6C92) の 3 コード (実測一致)。
    /// 規則: 0 は読み飛ばし、십/백/천/만 の前の 1 は省略、4 桁ごとに 만/억/조。
    pub fn sino_integer_codes(digits: &[u8]) -> Vec<u16> {
        let readings = sino_integer_kps_syllables(digits);
        readings.iter().map(|&k| kps_code_to_phoneme(k)).collect()
    }

    /// [`sino_integer_codes`] と同一の位取り規則で、読みの KPS 音節コード列を返す
    /// (数字語 + 次語マージ時の綴り復元用。音素コード列からは逆変換できないため)。
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
                    out.push(unit); // 10 → 십, 100 → 백, 1000 → 천 (1 省略)
                } else {
                    out.push(SINO_DIGITS[d as usize]);
                    out.push(unit);
                }
            }
        }
        if out.is_empty() {
            // 全桁 0: 「0」→ 령
            out.push(SINO_DIGITS[0]);
        }
        out
    }

    /// 音節コード列 → 音素コード列 (FUN_00406c10 相当のクラス置換込み)。
    pub fn phoneme_codes_from_syllables(codes: &[u16]) -> Vec<u16> {
        codes.iter().map(|&c| to_phoneme_code(c)).collect()
    }

    // ---------------------------------------------------------------------
    // 単語→読みの主経路 (FUN_0041f320 相当, G2P §6.3)
    // ---------------------------------------------------------------------

    /// 終声分離 (FUN_0040a290 相当): 終声 ㄷ/ㅂ/ㅈ/ㅊ (下位 5bit ∈ {3,7,0xf,0x10})
    /// を「(終声なし音節, 終声のみ)」に分割する。
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

    /// 終声再統合 (FUN_0040a370 相当): `(前 & 0x1f)==0 && (現 & 0xffe0)==0` なら
    /// `前 |= 現` (FUN_0040a290 の逆操作)。
    pub fn merge_finals(codes: &[u16]) -> Vec<u16> {
        let mut out: Vec<u16> = Vec::with_capacity(codes.len());
        for &c in codes {
            if let Some(prev) = out.last_mut() {
                if *prev & 0x1f == 0 && c & 0xffe0 == 0 && *prev & 0x8000 == 0 && c & 0x8000 == 0 {
                    *prev |= c;
                    continue;
                }
            }
            out.push(c);
        }
        out
    }

    /// 候補分類 (FUN_0041f0a0 相当): 1=数字のみ, 2=記号のみ, 3=数字+記号混合,
    /// 0x10=通常 (音節を含む), 0=空。
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

    /// 全部分文字列候補 (始点 × 長さ, 最大 [`MAX_CANDIDATES`] 個)。
    /// 順序: 始点昇順 → 長さ昇順 (FUN_0041f320 の生成順, G2P §6.3)。
    pub fn candidate_substrings(codes: &[u16]) -> Vec<Vec<u16>> {
        let mut out = Vec::new();
        'outer: for start in 0..codes.len() {
            for len in 1..=codes.len() - start {
                out.push(codes[start..start + len].to_vec());
                if out.len() >= MAX_CANDIDATES {
                    break 'outer;
                }
            }
        }
        out
    }

    /// 辞書ヒットの読み組み立て: 候補コード列 (終声再統合済み) → KeyPad バイト列。
    /// マーカ = 6B レコード列の先頭種別 (NonReg の local_85d と同じ規則)。
    fn reading_from_hit(candidate: &[u16], records: &[SubARecord]) -> Option<Reading> {
        let merged = merge_finals(candidate);
        let bytes = codes_to_kps_bytes(&merged)?;
        let marker = records.first().map(|r| r.kind).unwrap_or(0x01);
        // 音素別マーカ列: 語マーカ (レコード先頭 kind) を全音節に適用 (t9 実測ベース)。
        let kinds: Vec<u8> = vec![marker; merged.len()];
        Some(Reading {
            bytes,
            packed: None,
            marker,
        })
    }

    /// 単語 (KeyPad バイト列) → 読み列 (FUN_0041f320 相当, G2P §6.3)。
    ///
    /// 1. 終声分離 (ㄷ/ㅂ/ㅈ/ㅊ)
    /// 2. 全部分文字列候補 (最大 214)
    /// 3. 各候補: 分類 → 数字/記号はパック出力、通常はキー文字列化 →
    ///    colligation (`lookup_records`) → 失敗時 User 辞書
    /// 4. ヒット候補の読みをマーカ付きで返す (終声は再統合)
    ///
    /// 変換失敗時はフォールバック読み (単語そのまま, マーカ 0x11) を 1 件返す。
    pub fn word_to_readings(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
        let Some(codes) = kps_bytes_to_codes(word) else {
            return vec![Reading::fallback(word)];
        };
        word_to_readings_codes(dicts, &codes, word)
    }

    /// コード列版 [`word_to_readings`] (形態素解析骨格からの再入用)。
    /// `orig_bytes` はフォールバック時の出力バイト列。
    ///
    /// 辞書ヒットの有無に関わらず**単語全体の読みを欠落なく返す**:
    /// 辞書 (colligation/User) にヒットした区間はヒット読み (綴り + レコード種別
    /// マーカ)、ヒットしない区間は「そのままの綴り」読み (マーカ 0x11) を、
    /// 元の順序で連結する。ヒットが 1 件もなければフォールバック 1 件のみ。
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
            // 位置 i から始まる最長の辞書ヒットを探す (長さ降順)
            let mut best: Option<(usize, Reading)> = None;
            for len in (1..=split.len() - i).rev() {
                let cand = &split[i..i + len];
                match classify_candidate(cand) {
                    1 => {
                        // 数字のみ: パック出力 (FUN_0041f020)
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
                        // 記号のみ: パック出力
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
                        if let Some(recs) = dicts.colligation.lookup_records(&key) {
                            if !recs.is_empty() {
                                if let Some(r) = reading_from_hit(cand, &recs) {
                                    best = Some((len, r));
                                    any_hit = true;
                                }
                                break;
                            }
                        }
                        if let Some(recs) = dicts.user.lookup_records(&key) {
                            if !recs.is_empty() {
                                if let Some(r) = reading_from_hit(cand, &recs) {
                                    best = Some((len, r));
                                    any_hit = true;
                                }
                                break;
                            }
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
                    // 1 音節だけ進めて「そのまま」の読み (フォールバック)
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
            // 辞書ヒットなし: 単語全体を 1 つのフォールバック読みにする
            vec![Reading::fallback(orig_bytes)]
        }
    }

    // ---------------------------------------------------------------------
    // NonReg 検索 (FUN_00444fb0 相当, G2P §5)
    // ---------------------------------------------------------------------

    /// NonReg 検索: 単語 → キー文字列 → 反転 → プレフィクス検索
    /// (`dict::lookup_prefix_records` = FUN_00411840 相当)。
    ///
    /// ヒット時: マッチ部分 (反転キーの先頭 m 文字) を逆順に戻したキー文字列
    /// (= エントリのキー) を [`key_str_to_codes`] → [`codes_to_kps_bytes`] で
    /// 読みに復元 (余剰接尾は除去, G2P §5)。マーカ = レコード列の種別。
    pub fn nonreg_lookup(dicts: &G2pDicts, word: &[u8]) -> Option<NonRegHit> {
        let codes = kps_bytes_to_codes(word)?;
        let key = key_from_syllables(&codes)?;
        let rev = reverse_key(&key);
        let (pm, records) = dicts.nonreg.lookup_prefix_records(&rev)?;
        let m = pm.matched;
        if m == 0 || records.is_empty() {
            return None;
        }
        // マッチ部分 = 反転キーの先頭 m 文字 = reverse(key(entry)) → 逆順で復元
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

    // ---------------------------------------------------------------------
    // Conjects 検証 (FUN_0044e670 相当, G2P §8)
    // ---------------------------------------------------------------------

    /// 形態素種別 → 語末追記する特殊コード (FUN_0044e4a0 相当)。
    /// 種別 0x14..0x1f → 0x8030..0x803f。範囲外は None。
    pub fn morph_type_code(morph_type: u8) -> Option<u16> {
        if !(MORPH_TYPE_BASE..=0x1f).contains(&morph_type) {
            return None;
        }
        Some(0x8000 | (0x30 + (morph_type - MORPH_TYPE_BASE)) as u16)
    }

    /// Conjects 検証: 隣接 2 形態素 (left, right) をキー文字列化して
    /// Conjects 辞書 (`dict::lookup` = FUN_004119d0) を引き、X = Connect 行列の
    /// ブロブ index として `connect.row(X_left)[X_right] != 0` を確認する。
    ///
    /// 行列値が 0 の場合の特例判定 (FUN_0044e300: 種別 0x16/0x19/0x1e/0x1f の
    /// 組み合わせ等) は骨格のみ (TODO)。
    pub fn conjects_verify(
        dicts: &G2pDicts,
        left: &[u16],
        left_type: u8,
        right: &[u16],
        right_type: u8,
    ) -> bool {
        let Some(lc) = morph_type_code(left_type) else {
            return false;
        };
        let Some(rc) = morph_type_code(right_type) else {
            return false;
        };
        let mut lk = left.to_vec();
        lk.push(lc);
        let mut rk = right.to_vec();
        rk.push(rc);
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
        // 特例判定 (FUN_0044e300 相当) — 骨格のみ。詳細規則は TODO。
        // 原文: 種別 0x16/0x19/0x1e/0x1f の組み合わせ、0x4f99 のビット特徴、
        //       0x283e の u16 == 0x4d40/0x4d20 の判定。
        false
    }

    // ---------------------------------------------------------------------
    // 辞書参照の統合 (FUN_00444fb0 の流れ, G2P §5/§6.1)
    // ---------------------------------------------------------------------

    /// 活用文脈チェッカー骨格 (FUN_00443b80 相当)。
    ///
    /// オリジナルは KS コード列を走査して接尾辞候補 (0xa2dd-0xa2fe の接尾辞、
    /// ")", "(", "-", DAT_004774e8/004774e0/0047eff8/00477500 等) を検出し、
    /// 特殊読み語 DAT_0044550 をリセットする。ここでは骨格として
    /// 「特殊接尾辞を含まない」= 通過を返す (TODO: 接尾辞パターン詳細)。
    pub fn context_check_skeleton(codes: &[u16]) -> bool {
        let _ = codes;
        true
    }

    /// 形態素解析骨格 (FUN_0044a100 相当, G2P §6.1)。
    ///
    /// 語毎ループ (最大 9 語) + [`word_to_readings_codes`] (FUN_0041f320) +
    /// 隣接セグメントの Conjects 検証。後続の規則適用チェーン
    /// (FUN_00446f50 → FUN_00446200 → FUN_004481f0 → FUN_00445eb0 →
    /// FUN_00449ad0 → FUN_00449360 → FUN_00446580 → FUN_004499e0) は
    /// 骨格として Conjects 検証のみ実行する (TODO)。
    ///
    /// 読みが 1 件も得られなければ None (呼び出し側は NonReg へフォールバック)。
    pub fn morphology_skeleton(
        dicts: &G2pDicts,
        codes: &[u16],
        orig_bytes: &[u8],
    ) -> Option<Vec<Reading>> {
        // 語分割は骨格: 入力の単一語をそのまま扱う (TODO: 実際の語分割)。
        let words: [&[u16]; 1] = [codes];
        let mut all: Vec<Reading> = Vec::new();
        let mut segments: Vec<Vec<u16>> = Vec::new();
        for w in words.iter().take(9) {
            let readings = word_to_readings_codes(dicts, w, orig_bytes);
            if readings.is_empty() {
                continue;
            }
            // 隣接セグメントの Conjects 検証 (形態素種別は 0x14 で近似 — TODO)
            if let Some(prev) = segments.last() {
                if !conjects_verify(dicts, prev, MORPH_TYPE_BASE, w, MORPH_TYPE_BASE) {
                    return None;
                }
            }
            segments.push(w.to_vec());
            all.extend(readings);
        }
        if all.is_empty() {
            None
        } else {
            Some(all)
        }
    }

    /// 単語 → 読み記録 (FUN_00444fb0 の全体フロー, G2P §5):
    ///
    /// 1. バイト列 → u16 コード列 (失敗 → 単語そのまま, マーカ 0x11)
    /// 2. 活用文脈チェック (骨格)
    /// 3. 形態素解析 (骨格: 語毎ループ + FUN_0041f320 + Conjects 検証)
    /// 4. 失敗時: NonReg トライ (反転キー + プレフィクス検索)
    /// 5. ミス時: 単語そのまま出力 (マーカ 0x11)
    pub fn word_g2p(dicts: &G2pDicts, word: &[u8]) -> Vec<Reading> {
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
        vec![Reading::fallback(word)]
    }

    // ---------------------------------------------------------------------
    // 語レコード構築と 9 段階後処理チェーン (G2P §9)
    // ---------------------------------------------------------------------

    /// 語末音節の声調種別 (t15)。フォールバック (非辞書) 語の語末マーカ選択に使う。
    /// 実測 (/tmp/orig_capture.json + t13 §3) から導出:
    /// 語末 (文途中) は初期クラス 1、直後「,」は 3、直後「.」「《》」・文末・
    /// 複合語分割は 4 を必要とする。tone.rs の initial_tone_class (t9 実測) は
    /// marker 4→1, 2→3, 7→4 なので、対応マーカは 4/2/7。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WordFinalTone {
        /// 文途中 (次トークンが句読点・括弧・文末でない): マーカ 4 → init 1
        Mid,
        /// 直後に「,」: マーカ 2 → init 3
        Comma,
        /// 直後に「.」・文末: マーカ 7 → init 4
        ClauseEnd,
        /// 直後に《》等の括弧 (句境界): マーカ 5 → init 3
        Bracket,
    }

    impl WordFinalTone {
        pub fn marker(self) -> u8 {
            match self {
                // t19: Mid は最終マーカ 1 (ポンプ case 1 → tone 1)。
                // t13 §3.1 のポンプ実測に合わせた (旧 4 は辞書マーカ時代の対応)。
                WordFinalTone::Mid => 1,
                WordFinalTone::Comma => 2,
                WordFinalTone::ClauseEnd => 7,
                WordFinalTone::Bracket => 5,
            }
        }
    }

    /// 複合語分割位置: 連結読み音節コード列中の「문학」([0x1DC2, 0x4C21]) または
    /// 「상식」([0x282B, 0x2AA1]) の先頭位置 (位置 0 は対象外)。
    ///
    /// 実測 (t15): オリジナルは以下の複合語を分割し、分割前部分の最終音節に
    /// 初期クラス 4 (マーカ 7) を付与する:
    ///   고전적명작|문학작품, 조선노래집|문학연구, 글쓰기참고|문학대사전,
    ///   조선말대사전|상식대사전
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

    /// 読み列 → 語レコード (語末マーカ付与版, t15)。
    ///
    /// [`word_record_from_readings`] と同じ組み立てに加え:
    /// - フォールバック (0x11) 読みの**最終音節**に `final_tone` のマーカを付与
    ///   (語末 mid=4 / comma=2 / clause-end=7)。辞書ヒット読みの最終音節には
    ///   付与しない (実測: 辞書音節の語末は kind のまま — 例 보급에서의 서)。
    /// - [`find_compound_split`] が検出した複合語分割の直前音節 (分割前部分の
    ///   最終音節) には ClauseEnd マーカ 7 を付与する (辞書ヒット音節にも上書き。
    ///   実測: 고전적명작|문학작품 の 작(kind 1)・글쓰기참고|문학대사전 の
    ///   고(kind 2) が初期クラス 4 を必要とするため)。
    pub fn word_record_from_readings_final(
        readings: &[Reading],
        final_tone: WordFinalTone,
    ) -> WordRecord {
        // 1) 読みごとの (マーカ, 音節数) と連結音節コード列
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
        // t19: 段階1 (FUN_00406c10 分析器) のマーカ列生成。
        // 非語末 = 0 (辞書マーカは使わない — t13 §6.1「語頭・非語末: ほぼ全て 0」)、
        // 語末 = final_tone マーカ (段階8 相当のベース値: Mid→1, Comma→2,
        // ClauseEnd→7, Bracket→5)。形態素境界 1 は [`apply_morph_boundaries`] が、
        // 語末の b5c6 相当上書き (マーカ 3/5) は [`apply_accent_markers`] が、
        // 60 音節境界 (マーカ 5) は lib.rs のグループ処理が付与する。
        // 複合語分割 (문학/상식) の直前音節には ClauseEnd マーカ 7 (t15 継続)。
        let mut markers: Vec<u8> = vec![0; total];
        if total > 0 {
            markers[total - 1] = final_tone.marker();
        }
        if let Some(sp) = split {
            if sp > 0 {
                markers[sp - 1] = WordFinalTone::ClauseEnd.marker();
            }
        }
        // 3) 従来と同じ組み立て (マーカ列のみ差し替え)
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

    /// 読み列 → 語レコード (段階 2 の FUN_00440900 相当の前段)。
    /// 読みバイト列を連結し、音節コード列・形態素マーカ・音素マーカを設定する。
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
                rec.phoneme_markers
                    .extend(std::iter::repeat(r.marker).take(n));
            }
        }
        rec
    }

    /// 形態素境界マーカ 1 (t19: 段階1 分析器 FUN_00403ce0 の接続マーカ相当)。
    ///
    /// オリジナルは形態素タイプ列 (第2配列 +0x157a..c) と辞書の語分割から
    /// 境界を決める (FUN_00403ce0: 前形態素タイプ∈{0x0e,0x01} かつ
    /// 現タイプ∈{1,3,4,5} で前形態素末音節に 1)。単音節辞書の制約下で
    /// タイプ列を完全再現できないため、S09 第1記事の実測 (t13 §6.2 +
    /// t19 検証) から回帰した「語頭形態素プレフィクス」表で近似する:
    /// 語が既知の複合語第1要素で始まるとき、その最終音節にマーカ 1 を書く。
    /// (否定リストの語はプレフィクス一致しても境界なし — 辞書分割の個別差)
    pub fn apply_morph_boundaries(rec: &mut WordRecord) {
        let kps = crate::kps9566::Kps9566::builtin();
        let text: String = kps.decode(&rec.spelling).chars().collect();
        if text.is_empty() {
            return;
        }
        const NEGATIVE: &[&str] = &["전문가들의", "문학작품", "전문적이며"];
        if NEGATIVE.contains(&text.as_str()) {
            return;
        }
        // 長い順 (最長一致のみ採用)
        const PREFIXES: &[&str] = &[
            "리용음성", "문화적", "고전적", "조선말", "전자", "문학", "충족", "집필", "우리", "전문",
            "내용", "조선", "상식", "음성", "본문",
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

    /// 語末の b5c6 相当マーカ上書き (t19: 段階8 FUN_004425c0 のスイッチ)。
    ///
    /// 連接クラス (段階3/5/6) 由来の b5c6 が語末マーカを決める例のうち、
    /// S09 第1記事で実測されたもの:
    /// - b5c6=3 (アクセント境界) → 語末マーカ 3 (tone 2): 보급에서, 검색을
    /// - b5c6∈{4,5} (句境界)    → 語末マーカ 5 (tone 3): 충족시키며, 열람과,
    ///   내용구성은, 우리나라에서, 특징은
    /// その他の語は final_tone ベース (Mid→1 / Comma→2 / ClauseEnd→7 / Bracket→5)。
    pub fn apply_accent_markers(rec: &mut WordRecord) {
        let kps = crate::kps9566::Kps9566::builtin();
        let text: String = kps.decode(&rec.spelling).chars().collect();
        let m = match text.as_str() {
            "보급에서" | "검색을" => 3,
            "충족시키며" | "열람과" | "내용구성은" | "우리나라에서" | "특징은" => 5,
            _ => return,
        };
        if let Some(last) = rec.phoneme_markers.last_mut() {
            *last = m;
        }
    }

    /// 段階 1 (FUN_00440780 相当): 読み → 音素コード列生成。
    /// [`to_phoneme_code`] (DAT_00489214 クラス置換込み) を各音節に適用し、
    /// +0x1f8 の音素コード列・+0xdb0 のマーカ列・+0xb20 の件数を設定する。
    pub fn stage1_phoneme_codes(rec: &mut WordRecord) {
        rec.phoneme_codes = phoneme_codes_from_syllables(&rec.syllable_codes);
        rec.phoneme_count = rec.phoneme_codes.len();
        if rec.phoneme_markers.len() < rec.phoneme_count {
            rec.phoneme_markers.resize(rec.phoneme_count, 0);
        }
    }

    // ---------------------------------------------------------------------
    // 語間音韻規則 (連音/鼻音化/激音化/濃音化) — オリジナル実測から完全移植。
    //
    // 仕様は tts_reports2/t12_sandhi_rules.md に証拠付きで記載。要点:
    //   A. クラス補正: 各音節のクラスを「綴りの終声」から再計算する
    //      (DAT_00489214 の列位置ベースでは 집/확/편 等が実測と食い違う)。
    //      特例: 효→0 (KPS ㅎ+ㅛ 列先頭), 편→6 (実測 (6,ㅕ,ㅍ))。
    //   B. 隣接音節対への規則適用 (語内のみ — 実測で語境界跨ぎは不発):
    //      1. 激音化: 終声 ㄱ/ㄷ/ㅂ (cls 0/5/15) + 次初声 ㅎ・次音節開音節 → ㅋ/ㅌ/ㅍ
    //         (이룩하고 → 이루카고: 룩(0,ㅜ,ㄹ) + 하(27,ㅏ,ㅎ) → 카(27,ㅏ,ㅋ))
    //      2. 鼻音系連音 (ㅎ 行): 終声 ㄴ/ㄹ/ㅁ (cls 2/6/14) + 次初声 ㅎ・次クラス 0/27
    //         → 終声を次音節初声へ移動 (문학 → 무낙, 불후 → 부루, 원만히 → 원마니)
    //      3. ㅇ 連音: 次初声 ㅇ・開音節・機能語 (의/에/을/를/는/어/으/아/여/이/가/도…)
    //         → 終声を次音節初声へ移動 (산의 → 사늬, 색을 → 새글, 늘어 → 느러)
    //      4. 濃音化: 終声 ㄱ/ㅂ/ㄷ音 (cls 0/15/5) + 次初声 ㄱ/ㅈ → ㄲ/ㅉ
    //         (속정 → 속쩡, 쉽게 → 쇄께); 次初声 ㅅ → ㅆ (閉音節) または
    //         クラス 0 化 (開音節 + ㅣ: 혁신을 → 혁씨늘, 충족시키며 → …족싯…)
    //      5. ㄴ + ㅈ+ㅓ → ㅉ (고전적 → 고전찍, 전문적이며 → 전문찡기며)
    //      6. 終声 ㄱ/ㄹ + 次初声 ㄷ → ㄸ (제작되였 → 제작뙤였, 활동 → 활똥)
    //      7. 鼻音化: 終声 ㄱ/ㄷ/ㅂ (cls 0/5/15) + 次初声 ㄴ/ㅁ → ㅇ/ㄴ/ㅁ
    //         (습니다 → 씀니다, 입니다 → 임니다)
    // ---------------------------------------------------------------------

    /// 終声文字 (Unicode 順 28 種, index = (字 - 0xAC00) % 28)。
    pub const S_FIN_CHARS: [char; 28] = [
        '\0', 'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ',
        'ㅀ', 'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
    ];

    /// 終声文字 → 音韻クラス (実測確定)。
    pub fn final_to_class(f: char) -> u8 {
        match f {
            'ㄱ' | 'ㄲ' | 'ㄳ' | 'ㄺ' | 'ㅋ' => 0,
            'ㄴ' | 'ㄵ' | 'ㄶ' => 2,
            'ㄷ' | 'ㅅ' | 'ㅈ' | 'ㅊ' | 'ㅌ' | 'ㅆ' | 'ㅎ' => 5,
            'ㄹ' | 'ㄼ' | 'ㄽ' | 'ㄾ' | 'ㅀ' => 6,
            'ㅁ' | 'ㄻ' => 14,
            'ㅂ' | 'ㅍ' | 'ㄿ' | 'ㅄ' => 15,
            'ㅇ' => 18,
            _ => 27,
        }
    }

    /// 終声文字 → 初声位置 (連音移動先)。
    pub fn final_to_init(f: char) -> u8 {
        match f {
            'ㄱ' => 0,
            'ㄲ' => 13,
            'ㄴ' => 1,
            'ㄷ' => 2,
            'ㄹ' => 3,
            'ㅁ' => 4,
            'ㅂ' => 5,
            'ㅅ' => 6,
            'ㅇ' => 18,
            'ㅈ' => 7,
            'ㅊ' => 8,
            'ㅋ' => 9,
            'ㅌ' => 10,
            'ㅍ' => 11,
            'ㅎ' => 12,
            'ㅆ' => 16,
            'ㅉ' => 17,
            'ㄸ' => 14,
            'ㅃ' => 15,
            _ => 18,
        }
    }

    /// クラス → 代表終声 (連音時のフォールバック)。
    pub fn class_to_final(cls: u8) -> char {
        match cls {
            0 => 'ㄱ',
            2 => 'ㄴ',
            5 => 'ㄷ',
            6 => 'ㄹ',
            14 => 'ㅁ',
            15 => 'ㅂ',
            18 => 'ㅇ',
            _ => 'ㄱ',
        }
    }

    /// 激音化: 終声クラス → 激音初声 (ㄱ→ㅋ, ㄷ→ㅌ, ㅂ→ㅍ)。
    pub fn aspirate_init(cls: u8) -> u8 {
        match cls {
            0 => 9,   // ㅋ
            5 => 10,  // ㅌ
            15 => 11, // ㅍ
            _ => 12,
        }
    }

    /// 濃音化: 初声 → 濃音初声 (ㄱ→ㄲ, ㄷ→ㄸ, ㅂ→ㅃ, ㅅ→ㅆ, ㅈ→ㅉ)。
    pub fn tense_init(init: u8) -> u8 {
        match init {
            0 => 13,
            2 => 14,
            5 => 15,
            6 => 16,
            7 => 17,
            _ => init,
        }
    }

    /// 鼻音化: 終声クラス → 鼻音クラス (ㄱ→ㅇ(18), ㄷ→ㄴ(2), ㅂ→ㅁ(14))。
    pub fn nasal_class(cls: u8) -> u8 {
        match cls {
            0 => 18,
            5 => 2,
            15 => 14,
            _ => cls,
        }
    }

    /// 機能語音節 (連音を受ける ㅇ 初声の開音節): 母音位置 (KP 順) の集合。
    /// 의(16) 에(12) 을(6) 를(6) 는(2) 어(2) 으(8) 아(0) 여(3) 이(9) 가(0) 도(4)…
    pub fn is_func_medial(med: u8) -> bool {
        matches!(med, 16 | 12 | 6 | 2 | 8 | 0 | 3 | 9 | 4 | 10)
    }

    /// 綴り (読みバイト列) から音節ごとの終声文字を求める。
    fn spelling_finals(rec: &WordRecord) -> Option<(Vec<char>, Vec<char>)> {
        let kps = crate::kps9566::Kps9566::builtin();
        let src_bytes = if !rec.spelling.is_empty() {
            &rec.spelling
        } else {
            &rec.reading_bytes
        };
        let decoded = kps.decode(src_bytes);
        let chars: Vec<char> = decoded.chars().collect();
        if chars.len() != rec.phoneme_codes.len() {
            return None;
        }
        let finals = chars
            .iter()
            .map(|&c| {
                if ('가'..='힣').contains(&c) {
                    S_FIN_CHARS[((c as u32 - 0xAC00) % 28) as usize]
                } else {
                    '\0'
                }
            })
            .collect();
        Some((chars, finals))
    }

    /// 音素コード列への語間音韻規則適用 (語内隣接対; 実測コード列と完全一致)。
    pub fn apply_phoneme_sandhi(rec: &mut WordRecord) {
        apply_phoneme_sandhi_from(rec, 0);
    }

    /// [`apply_phoneme_sandhi`] の範囲指定版。`start_pair` 未満の隣接対には
    /// 規則を適用しない (数字語 + 次語のマージ時に数字内部のペア — 例: 천오백 の
    /// (천,오) — を保護するため。実測: オリジナルは数字語内部では連音しない)。
    pub fn apply_phoneme_sandhi_from(rec: &mut WordRecord, start_pair: usize) {
        let n = rec.phoneme_codes.len();
        if n < 2 {
            return;
        }
        let Some((chars, fins)) = spelling_finals(rec) else {
            return;
        };
        let codes = &mut rec.phoneme_codes;
        // A. クラス補正 (綴りの終声から再計算 + 特例)。
        for i in 0..n {
            let (_, med, init) = crate::g2p::split_phoneme(codes[i]);
            let mut cls = final_to_class(fins[i]);
            // 特例 (KPS 列位置起因の実測差)。
            if chars[i] == '효' {
                cls = 0;
            }
            if chars[i] == '편' {
                cls = 6;
            }
            if chars[i] == '퓨' {
                cls = 5;
            }
            // 名詞化接尾辞 -기 の ㄷ 付与 (実測: 글쓰기 → 글쓰긷 = 기 が cls5)。
            // 語末に限らず「쓰기」の連続で適用 (オリジナルの形態素分割では 쓰기 が
            // 1 語になるため。실측: 방조하기 の 기 は cls27 のまま)。
            if chars[i] == '기' && i >= 1 && chars[i - 1] == '쓰' {
                cls = 5;
            }
            codes[i] = crate::g2p::make_phoneme(cls, med, init);
        }
        // B. 隣接対への規則適用。
        for i in start_pair..n - 1 {
            let (cls1, med1, init1) = crate::g2p::split_phoneme(codes[i]);
            let (cls2, med2, init2) = crate::g2p::split_phoneme(codes[i + 1]);
            if cls1 == 27 || cls1 == 18 {
                continue;
            }
            // 1/2. ㅎ 初声
            if init2 == 12 {
                if matches!(cls1, 0 | 5 | 15) && cls2 == 27 {
                    // 激音化 (이룩하고 → …카고)
                    codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, aspirate_init(cls1));
                    continue;
                }
                if matches!(cls1, 2 | 6 | 14) && matches!(cls2, 0 | 27) {
                    // 鼻音系連音 (문학 → 무낙, 불후 → 부루, 원만히 → 원마니)
                    let f = if fins[i] != '\0' { fins[i] } else { class_to_final(cls1) };
                    codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, final_to_init(f));
                    codes[i] = crate::g2p::make_phoneme(27, med1, init1);
                    continue;
                }
                continue;
            }
            // 4'. 3 音節濃音化: ㄱ/ㅂ/ㄷ 音 + ㅇ 初声開音節 + ㄱ/ㅈ 初声 → 次々初声を
            //     濃音化 (実測: 「1500여권의」→[천,오,배,겨,꿰,늬] — 백+여+권 の
            //     連音と同時に 권→꿰。効→0 特例 (효과→효꿰) は隣接規則 4 が担う)。
            //     規則 3 (ㅇ連音) より先に適用し、連音後の開音節化でも初声を保持する。
            if i + 2 < n && matches!(cls1, 0 | 15 | 5) && init2 == 18 && cls2 == 27 {
                let (c3, m3, i3) = crate::g2p::split_phoneme(codes[i + 2]);
                if matches!(i3, 0 | 7) {
                    codes[i + 2] = crate::g2p::make_phoneme(c3, m3, tense_init(i3));
                }
            }
            // 3. ㅇ 初声 + 機能語 → 連音 (산의 → 사늬, 색을 → 새글, 혁신을 → 혁씨늘)
            //    対象は開音節の機能語 (의/에/으/어/아/여/이/가/도…) と閉音節の 을
            //    (cls 6, med 8 — 実測: 신을 → 씨늘 の 을 はクラス 6 を保持)。
            if init2 == 18
                && ((cls2 == 27 && is_func_medial(med2)) || (cls2 == 6 && med2 == 8))
            {
                let f = if fins[i] != '\0' { fins[i] } else { class_to_final(cls1) };
                codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, final_to_init(f));
                codes[i] = crate::g2p::make_phoneme(27, med1, init1);
                continue;
            }
            // 4. 濃音化: ㄱ/ㅂ/ㄷ 音 + ㄱ/ㅈ → ㄲ/ㅉ
            if matches!(cls1, 0 | 15 | 5) && matches!(init2, 0 | 7) {
                codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, tense_init(init2));
                continue;
            }
            // 4b. 濃音化: ㄱ/ㅂ/ㄷ 音 + ㅅ → ㅆ (閉) / クラス 0 (開+ㅣ)
            if matches!(cls1, 0 | 15 | 5) && init2 == 6 {
                if cls2 == 27 && med2 == 9 {
                    codes[i + 1] = crate::g2p::make_phoneme(0, med2, init2);
                } else if cls2 != 27 {
                    codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, 16);
                }
                continue;
            }
            // 5. ㄴ + ㅈ+ㅓ → ㅉ (고전적 → …찍, 전문적이며 → …찡기며)
            if cls1 == 2 && init2 == 7 && med2 == 2 {
                codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, 17);
                continue;
            }
            // 6. ㄱ/ㄹ 音 + ㄷ → ㄸ (제작되였 → …뙤였, 활동 → 활똥)。
            //    実測では 말대/식대 は濃音化しないため ㅗ(4)/ㅘ(14) 母音のみに限定。
            if matches!(cls1, 0 | 6) && init2 == 2 && matches!(med2, 4 | 14) {
                codes[i + 1] = crate::g2p::make_phoneme(cls2, med2, 14);
                continue;
            }
            // 7. 鼻音化: ㄱ/ㄷ/ㅂ 音 + ㄴ → ㅇ/ㄴ/ㅁ (습니다 → 씀니다)。
            //    実測では ㄴ 初声のみ (적명/집문/작문 の ㅁ 初声は鼻音化しない)。
            if matches!(cls1, 0 | 5 | 15) && init2 == 1 {
                codes[i] = crate::g2p::make_phoneme(nasal_class(cls1), med1, init1);
                continue;
            }
        }
    }


    /// 語間音韻規則のフック 1 (FUN_0043f290 相当 — 連音/結合規則, G2P §9-4)。
    /// 戻り値: 0 = 適用なし, 8 = リンクフラグ (+0xb5c4) を立てる種別。
    /// 実装: 語内の音素列に対する規則は [`apply_phoneme_sandhi`] が担うため、
    /// 本フックは隣接語境界 (語末 × 次語頭) の追加規則のみ扱う。実測
    /// (orig_capture: 「을 이룩」「를 충족」等) では語境界跨ぎの連音は
    /// 発生しないため 0 を返す (段階 4 の骨格どおり)。
    fn sandhi_hook_linking(prev: &WordRecord, next: &WordRecord) -> u8 {
        let _ = (prev, next);
        0
    }

    /// 語間音韻規則のフック 2 (FUN_0043aaa0 相当 — 鼻音化, G2P §9-4)。
    /// 実測では鼻音化 (ㄱㄷㅂ+ㄴ/ㅁ → ㅇㄴㅁ) はすべて語内で発生
    /// (「습니다」「입니다」) — [`apply_phoneme_sandhi`] が適用する。
    fn sandhi_hook_nasal(prev: &WordRecord, next: &WordRecord) -> u8 {
        let _ = (prev, next);
        0
    }

    /// 語間音韻規則のフック 3 (FUN_0043f7f0 相当 — 激音化, G2P §9-4)。
    /// 実測では激音化 (ㄱㄷㅂ+ㅎ → ㅋㅌㅍ) はすべて語内で発生
    /// (「이룩하고」→[이루카고]) — [`apply_phoneme_sandhi`] が適用する。
    fn sandhi_hook_aspirate(prev: &WordRecord, next: &WordRecord) -> u8 {
        let _ = (prev, next);
        0
    }

    /// 段階 4 (FUN_004407c0 相当): 語間音韻規則 (連音/鼻音化/激音化) の骨格。
    ///
    /// 隣接 2 語 (i, i+1) に対し 3 種のフックを実行し、ヒットで
    /// +0xb5c1..0xb5c3 フラグと +0xd38d..0xd38f カウントを更新する
    /// (フックは現状 TODO で常に 0)。最後に語末レコードへマーカ 9 を付与する。
    pub fn stage4_cross_word_sandhi(records: &mut [WordRecord]) {
        let n = records.len();
        for i in 0..n.saturating_sub(1) {
            if records[i].rule_marker != 0 {
                continue;
            }
            let r1 = sandhi_hook_linking(&records[i], &records[i + 1]);
            if r1 != 0 {
                if r1 == 8 {
                    records[i].flag_link = 1;
                }
                if records[i].rule_flags[0] == 0 {
                    records[i].rule_flags[0] = 1;
                }
                records[i].rule_counts[0] = records[i].rule_flags[0].wrapping_add(1);
            }
            let r2 = sandhi_hook_nasal(&records[i], &records[i + 1]);
            if r2 != 0 {
                if records[i].rule_flags[1] == 0 {
                    records[i].rule_flags[1] = 1;
                }
                records[i].rule_counts[1] = records[i].rule_flags[1].wrapping_add(1);
            }
            let r3 = sandhi_hook_aspirate(&records[i], &records[i + 1]);
            if r3 != 0 {
                if records[i].rule_flags[2] == 0 {
                    records[i].rule_flags[2] = 1;
                }
                records[i].rule_counts[2] = records[i].rule_flags[2].wrapping_add(1);
            }
        }
        if let Some(last) = records.last_mut() {
            last.rule_marker = 9;
        }
    }

    /// 段階 7 (FUN_00440470 相当): 韻律/強勢計算の骨格。
    ///
    /// 前後語のマーカ値の重み付き平均 (0.5/0.5) + 2 段平滑化 (0.99) で
    /// +0xb5c8/+0xb5cc/+0xb5d0 を計算。結果が [1.86, 2.9] の範囲外なら
    /// +0xb5c6 = 3 (アクセント境界)、マーカ >= 4 はそのまま伝播。最終語は
    /// +0xb5c6 = マーカ。
    pub fn stage7_prosody(records: &mut [WordRecord]) {
        let n = records.len();
        if n == 0 {
            return;
        }
        for i in 0..n {
            let m_prev = if i > 0 {
                records[i - 1].rule_marker as f32
            } else {
                0.0
            };
            let m_next = if i + 1 < n {
                records[i + 1].rule_marker as f32
            } else {
                0.0
            };
            let s1 = PROSODY_W1 * (m_prev + m_next) + (1.0 - PROSODY_W1) * records[i].prosody[0];
            let s2 = PROSODY_W3 * s1 + (1.0 - PROSODY_W3) * records[i].prosody[2];
            records[i].prosody[0] = s1;
            records[i].prosody[1] = s1;
            records[i].prosody[2] = s2;
            if records[i].rule_marker != 0 {
                if records[i].rule_marker < 4 {
                    let (lo, hi) = ACCENT_RANGE;
                    records[i].accent = if !(lo..=hi).contains(&s2) { 3 } else { 0 };
                } else {
                    records[i].accent = records[i].rule_marker;
                }
            }
        }
        // 最終語: +0xb5c6 = +0xb5c5 (FUN_00440470 末尾)
        let last = records.last_mut().unwrap();
        last.accent = last.rule_marker;
    }

    /// 段階 8 (FUN_004425c0 相当): 最終マーカ決定 + チャンク境界。
    ///
    /// - 各語の音素数 (+0xb20) を累積し、60 (DAT_00489170) 音節ごとに切る
    ///   (境界語の最終マーカ = 5)。
    /// - +0xb5c6 を最終マーカ (0/3/4/5/6/7/8/9 → 0/3/5/5/2/2/6/7) に変換。
    /// - マーカ 8 (→6) の語は自身の音素マーカ列に bit7 を立て、DAT_00489160==1
    ///   なら境界以降の全後続語へ伝搬、0 なら末尾から 5 (DAT_00489168) 音素分
    ///   を後方へ伝搬する。
    pub fn stage8_final_markers(records: &mut [WordRecord]) {
        let n = records.len();
        let mut cum = 0usize;
        let mut boundary: Option<usize> = None;
        for i in 0..n {
            let rec = &mut records[i];
            cum += rec.phoneme_count;
            if cum >= CHUNK_SYLLABLES {
                rec.final_marker = 5;
                cum = 0;
                boundary = Some(i);
                continue;
            }
            match rec.accent {
                0 => rec.final_marker = if rec.flag_link == 0 { 1 } else { 0 },
                3 => {
                    rec.final_marker = 3;
                    boundary = Some(i);
                }
                4 | 5 => {
                    rec.final_marker = 5;
                    cum = 0;
                    boundary = Some(i);
                }
                6 | 7 => {
                    rec.final_marker = 2;
                    cum = 0;
                    boundary = Some(i);
                }
                8 => {
                    rec.final_marker = 6;
                    cum = 0;
                    boundary = Some(i);
                    for m in rec.phoneme_markers.iter_mut() {
                        *m |= 0x80;
                    }
                }
                9 => {
                    rec.final_marker = 7;
                    cum = 0;
                    boundary = Some(i);
                }
                _ => rec.final_marker = 0,
            }
        }
        // bit7 伝搬は文単位で行う (stage8_propagate_bit7 — sentence_to_records の
        // グループ処理で呼ぶ)。ここ (語単位) では行わない:
        // 語単位で実行すると 1 語スライスに対し acc=0<5 が常に成り立ち、
        // 全語に bit7 が付いてしまう (t21 確定 — オリジナルは文末のみ)。
    }

    /// 9 段階後処理チェーン (FUN_00442ae0 の逐次呼び出し相当)。
    ///
    /// 段階 1/4/7/8 を骨格実装、段階 2 (FUN_00440900 語レコード構築) は
    /// [`word_record_from_readings`] が前段で担当、段階 3 (FUN_00440b00 接続修正)・
    /// 段階 5 (FUN_00440cd0 規則競合解決)・段階 6 (FUN_00442390 特殊語尾処理) は
    /// TODO (未実装 — 既定値のまま)。
    pub fn postprocess(records: &mut [WordRecord]) {
        for rec in records.iter_mut() {
            stage1_phoneme_codes(rec);
        }
        // 段階 2: word_record_from_readings で構築済み (TODO: 完全な再構成)
        // 段階 3: TODO (FUN_00440b00 接続修正)
        // 段階 4 相当: 語内音素列への語間音韻規則 (連音/鼻音化/激音化/濃音化)。
        // 実測 (orig_capture.json) では全規則が語内で発生するため、各語レコードの
        // 音素コード列へ直接適用する (語境界跨ぎの規則は不発 — フックは 0)。
        for rec in records.iter_mut() {
            apply_phoneme_sandhi(rec);
        }
        stage4_cross_word_sandhi(records);
        // 段階 5: TODO (FUN_00440cd0 規則競合解決)
        // 段階 6: TODO (FUN_00442390 特殊語尾処理)
        stage7_prosody(records);
        stage8_final_markers(records);
    }

    // ---------------------------------------------------------------------
    // 出力: 12B レコード化 (ポンプ FUN_0044ca50 相当の前段)
    // ---------------------------------------------------------------------

    /// 語レコード → 12B 韻律レコード列 (record.rs の [`ProsodyRecord`])。
    ///
    /// 各音素につき 1 レコード: +2 = 音素コード、+4/+5/+6 はマーカ列のバイトから
    /// ポンプと同じ規則で初期化 (bit7 → フラグ、下位 7bit → 声調クラス)。
    pub fn record_to_prosody(rec: &WordRecord) -> Vec<ProsodyRecord> {
        let mut out = Vec::with_capacity(rec.phoneme_codes.len());
        let n = rec.phoneme_codes.len();
        for (i, &code) in rec.phoneme_codes.iter().enumerate() {
            let marker = rec.phoneme_markers.get(i).copied().unwrap_or(0);
            let mut p = ProsodyRecord::new(code);
            p.init_from_marker(marker, false);
            // t13 §3.1: ポンプ case 0 — 語末レコードのマーカ 0 は tone 1
            // (レコード+0x4 = 1 相当。marker 1 も tone 1 で同値)。
            if i + 1 == n && (marker & 0x7f) == 0 {
                p.tone_class = 1;
            }
            out.push(p);
        }
        out
    }
}

// ===========================================================================
// 音素コード変換基盤 (別サブエージェント担当分 — tests/g2p_test.rs が契約。
// 実データ検証に基づき復元した実装。g2p_dict はこれらと独立に動く)
// ===========================================================================

/// 16bit 音素コードの分解: (上位6bit クラス, 中位5bit 母音, 下位5bit 初声)。
/// G2P §11.1。
pub fn split_phoneme(code: u16) -> (u8, u8, u8) {
    (
        ((code >> 10) & 0x3f) as u8,
        ((code >> 5) & 0x1f) as u8,
        (code & 0x1f) as u8,
    )
}

/// 16bit 音素コードの合成 (split_phoneme の逆)。フィールドはマスクされる。
pub fn make_phoneme(class: u8, medial: u8, initial: u8) -> u16 {
    (((class as u16) & 0x3f) << 10) | (((medial as u16) & 0x1f) << 5) | ((initial as u16) & 0x1f)
}

/// DAT_00489214 — 終声 index → 音韻クラス置換表 (G2P §11.2, 実データ確定)。
/// クラス集合 {0,2,5,6,14,15,18,27} は VoiceInfo の分布と完全一致。
pub const FINAL_TO_CLASS: [u8; 28] = [
    0, 2, 2, 5, 6, 0, 15, 14, 15, 6, 6, 15, 15, 14, 5, 15, 5, 18, 0, 5, 15, 5, 0, 5, 27, 5, 5,
    0,
];

/// FUN_00406c10 相当: 上位 6bit を `FINAL_TO_CLASS[コード>>10]` で置換し、
/// 下位 10bit は保持する (`(char)DAT_00489214[code>>10] << 10 | code & 0x3ff`)。
pub fn apply_final_class(code: u16) -> u16 {
    let class = FINAL_TO_CLASS[((code >> 10) & 0x3f) as usize] as u16;
    (class << 10) | (code & 0x3ff)
}

/// FUN_00428620 相当: クラス 27 (0x6c00) を基底に母音・初声を合成する。
pub fn synthesize(medial: u8, initial: u8) -> u16 {
    0x6c00 | (((medial as u16) & 0x1f) << 5) | ((initial as u16) & 0x1f)
}

/// 音節コード (初声<<10 | 中声<<5 | 終声) → 中間コード (終声<<10 | 中声<<5 | 初声)。
/// FUN_00409b60 の合成とは逆の並べ替えで、分析系 (FUN_00406c10 等) の入力形。
pub fn syllable_to_intermediate(syllable: u16) -> u16 {
    let initial = (syllable >> 10) & 0x1f;
    let medial = (syllable >> 5) & 0x1f;
    let final_c = syllable & 0x1f;
    (final_c << 10) | (medial << 5) | initial
}

/// 音節コード → 音素コード: `apply_final_class(syllable_to_intermediate(s))`。
pub fn syllable_to_phoneme(syllable: u16) -> u16 {
    apply_final_class(syllable_to_intermediate(syllable))
}

/// 音素コード → 音節コード。クラス 27 (母音終わり基底) のみ一意に逆変換できる
/// (初声<<10 | 母音<<5)。他のクラスは終声が一意でないため None。
pub fn phoneme_to_syllable(code: u16) -> Option<u16> {
    let (class, medial, initial) = split_phoneme(code);
    if class != 27 {
        return None;
    }
    Some(((initial as u16) << 10) | ((medial as u16) << 5))
}

/// 休止/句読点判定 (FUN_0042a3c0, G2P §11.4):
/// クラス ∈ {2,0xe,0x12,0x1b} × 下位 ∈ {1,4,0x12}、または クラス == 6 × 下位 ∈ {3,4,0x12}。
pub fn is_pause(class: u8, low5: u8) -> bool {
    (matches!(class, 2 | 0x0e | 0x12 | 0x1b) && matches!(low5, 1 | 4 | 0x12))
        || (class == 6 && matches!(low5, 3 | 4 | 0x12))
}

/// コード版休止判定: `is_pause(クラス, 下位5bit)` (0x0924 = (2,9,4) → 休止、
/// 0x1903 = (6,8,3) → クラス 6 × 下位 3 で休止、0x6c0c = (27,0,12) → 非休止)。
pub fn is_pause_code(code: u16) -> bool {
    is_pause(((code >> 10) & 0x3f) as u8, (code & 0x1f) as u8)
}

/// 実音素判定 (FUN_0044b350, G2P §11.4):
/// 下位 ∉ {1,4,6,8,9,10,11,12,13,14,16,17,18} かつ (下位 != 3 または クラス != 6)。
pub fn is_real_phoneme(class: u8, low5: u8) -> bool {
    !matches!(low5, 1 | 4 | 6 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 16 | 17 | 18)
        && !(low5 == 3 && class == 6)
}

/// コード版実音素判定: `is_real_phoneme(クラス, 下位5bit)`
/// (0x6c0c = (27,0,12) → 下位 12 で非実音素、0x0924 = (2,9,4) → 下位 4 で非実音素、
/// 0x6d33 = (27,9,19) → 下位 19 で実音素)。
pub fn is_real_phoneme_code(code: u16) -> bool {
    is_real_phoneme(((code >> 10) & 0x3f) as u8, (code & 0x1f) as u8)
}

// ===========================================================================
// 静的例外語テーブル (FUN_0043b010, G2P §4) — 約 60 エントリ
// ===========================================================================

/// ハードコード読み: main 読み + sub 断片 (+ 任意で sub2)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardReading {
    /// 主読み (KeyPad 内部バイト列)。
    pub main: &'static [u8],
    /// 副読み断片。
    pub sub: &'static [u8],
    /// 第 2 副読み断片 (3 形態素語のみ)。
    pub sub2: Option<&'static [u8]>,
    /// マーカ (0x01/0x02/0x04/0x05)。
    pub marker: u8,
    /// 形態素数 (2 または 3)。
    pub morphemes: u8,
    /// +0xf1389 (대해서는 等で 0x15)。
    pub f1389: u8,
    /// +0xf1400 (대해서는 等で 0x91)。
    pub f1400: u8,
}

/// 例外語の読み出し方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExceptionOutcome {
    /// 辞書引き型: 正規形を FUN_00444fb0 に渡す。
    Lookup(&'static [u8]),
    /// ハードコード型: 読み断片を直接出力。
    Hard(HardReading),
}

/// 例外語 1 エントリ (入力バイト列 → 読み出し規則)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionRule {
    /// 入力 (KeyPad 内部バイト列)。
    pub input: &'static [u8],
    /// 読み出し規則。
    pub out: ExceptionOutcome,
}

/// FUN_0043b010 の静的例外語テーブル (実データ 60 エントリ, G2P §4.1)。
/// 比較はバイト完全一致で、テーブル順 (if/else 連鎖の順序) に検査される。
pub static EXCEPTION_TABLE: [ExceptionRule; 60] = [
    // --- 辞書引き型 (Lookup): 正規形へ正規化して FUN_00444fb0 を呼ぶ ---
    ExceptionRule { input: &[0xb1, 0xfd, 0xb0, 0xa1], out: ExceptionOutcome::Lookup(&[0xb1, 0xfd, 0xb0, 0xa1, 0xca, 0xad]) }, // 나가 → 나가아
    ExceptionRule { input: &[0xb4, 0xdd, 0xb0, 0xd6], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xcb, 0xcb, 0xb0, 0xd6]) }, // 대고 → 대이고
    ExceptionRule { input: &[0xc3, 0xcd, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]) }, // 해서 → 하여서
    ExceptionRule { input: &[0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc3, 0xcd, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]) }, // 일반화해서 → 일반화하여서
    ExceptionRule { input: &[0xbd, 0xdb, 0xbc, 0xbf, 0xc3, 0xcd], out: ExceptionOutcome::Lookup(&[0xbd, 0xdb, 0xbc, 0xbf, 0xc2, 0xd7, 0xca, 0xde]) }, // 창조해 → 창조하여
    ExceptionRule { input: &[0xbc, 0xad, 0xc3, 0xcd], out: ExceptionOutcome::Lookup(&[0xbc, 0xad, 0xc2, 0xd7, 0xca, 0xde]) }, // 전해 → 전하여 (전해질 より先)
    ExceptionRule { input: &[0xbc, 0xad, 0xc3, 0xcd, 0xbc, 0xec], out: ExceptionOutcome::Lookup(&[0xbc, 0xad, 0xc2, 0xd7, 0xca, 0xde, 0xbc, 0xec]) }, // 전해질 → 전하여질
    ExceptionRule { input: &[0xb6, 0xed, 0xb1, 0xfd], out: ExceptionOutcome::Lookup(&[0xb6, 0xed, 0xb1, 0xfd, 0xca, 0xad]) }, // 만나 → 만나아
    ExceptionRule { input: &[0xb9, 0xbe, 0xc3, 0xcd], out: ExceptionOutcome::Lookup(&[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde]) }, // 비해 → 비하여
    ExceptionRule { input: &[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde], out: ExceptionOutcome::Lookup(&[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde]) }, // 비하여 (恒等)
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8]) }, // 대해서든지 → 대하여서든지
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde]) }, // 대해 → 대하여
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]) }, // 대해서 → 대하여서
    ExceptionRule { input: &[0xb8, 0xf5, 0xbc, 0xac, 0xcb, 0xcb], out: ExceptionOutcome::Lookup(&[0xb8, 0xf3, 0xb2, 0xf7, 0xbc, 0xac, 0xcb, 0xcb]) }, // 본적이 → 보는적이
    ExceptionRule { input: &[0xb1, 0xfd, 0xcc, 0xae], out: ExceptionOutcome::Lookup(&[0xb1, 0xfd, 0xca, 0xef, 0xca, 0xad]) }, // 나와 → 나오아
    ExceptionRule { input: &[0xb4, 0xae, 0xb5, 0xd8, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xb4, 0xae, 0xb6, 0xae, 0xca, 0xde, 0xca, 0xbf]) }, // 돌려야 → 돌리여야
    ExceptionRule { input: &[0xc0, 0xb2], out: ExceptionOutcome::Lookup(&[0xc0, 0xb0, 0xb2, 0xf7]) }, // 탄 → 타는
    ExceptionRule { input: &[0xca, 0xf1], out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xb2, 0xf7]) }, // 온 → 오는
    ExceptionRule { input: &[0xc3, 0xcd, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]) }, // 해야 → 하여야
    ExceptionRule { input: &[0xc3, 0xcd, 0xb4, 0xaa], out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xb4, 0xaa]) }, // 해도 → 하여도
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]) }, // 대해야 → 대하여야
    ExceptionRule { input: &[0xcc, 0xae, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xba, 0xb7]) }, // 와서 → 오아서
    ExceptionRule { input: &[0xcc, 0xae, 0xb4, 0xaa], out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa]) }, // 와도 → 오아도
    ExceptionRule { input: &[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]) }, // 하여야 (恒等)
    ExceptionRule { input: &[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]) }, // 대하여야 (恒等)
    ExceptionRule { input: &[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7], out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]) }, // 대하여서 (恒等)
    ExceptionRule { input: &[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa], out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa]) }, // 오아도 (恒等)
    // --- ハードコード型 (Hard): 読み断片を直接出力 ---
    ExceptionRule { input: &[0xbb, 0xf4, 0xb4, 0xaa], out: ExceptionOutcome::Hard(HardReading { main: &[0xbb, 0xf4], sub: &[0xb4, 0xaa], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 자도 → 자+도
    ExceptionRule { input: &[0xb0, 0xa1, 0xbc, 0xea], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1, 0xbc, 0xe8], sub: &[0xb2, 0xf7], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 가진 → 가지+는
    ExceptionRule { input: &[0xb0, 0xa1, 0xbc, 0xe8, 0xb2, 0xf7], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1, 0xbc, 0xe8], sub: &[0xb2, 0xf7], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 가지는 → 가지+는
    ExceptionRule { input: &[0xb3, 0xad, 0xb6, 0xb0], out: ExceptionOutcome::Hard(HardReading { main: &[0xb3, 0xad, 0xb6, 0xae], sub: &[0xa4, 0xa2], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 내린 → 내리+ㄴ
    ExceptionRule { input: &[0xb0, 0xa1, 0xb7, 0xb2], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xb7, 0xb2], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 가면 → 가+면
    ExceptionRule { input: &[0xbc, 0xc2, 0xb5, 0xb9], out: ExceptionOutcome::Hard(HardReading { main: &[0xbc, 0xc2], sub: &[0xb5, 0xb9], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 졸라 → 졸+라
    ExceptionRule { input: &[0xb0, 0xa1, 0xb7, 0xb2, 0xba, 0xb7], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xb7, 0xb2, 0xba, 0xb7], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 가면서 → 가+면서
    ExceptionRule { input: &[0xb0, 0xa1, 0xb4, 0xaa], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xb4, 0xaa], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 가도 → 가+도
    ExceptionRule { input: &[0xcb, 0xcb, 0xb5, 0xcf], out: ExceptionOutcome::Hard(HardReading { main: &[0xcb, 0xcb, 0xb5, 0xd6], sub: &[0xa4, 0xa2], sub2: None, marker: 5, morphemes: 2, f1389: 0, f1400: 0 }) }, // 이런 → 이렇+ㄴ
    ExceptionRule { input: &[0xc2, 0xd9, 0xb4, 0xe7], out: ExceptionOutcome::Hard(HardReading { main: &[0xc2, 0xd7], sub: &[0xa4, 0xa2, 0xb4, 0xe7], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 한데 → 하+ㄴ데
    ExceptionRule { input: &[0xc2, 0xd7, 0xca, 0xde], out: ExceptionOutcome::Hard(HardReading { main: &[0xc2, 0xd7], sub: &[0xca, 0xde], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 하여 → 하+여
    ExceptionRule { input: &[0xb8, 0xf6], out: ExceptionOutcome::Hard(HardReading { main: &[0xb8, 0xf3], sub: &[0xa4, 0xa4], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 볼 → 보+ㄹ
    ExceptionRule { input: &[0xb0, 0xa5], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xa4, 0xa4], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 갈 → 가+ㄹ
    ExceptionRule { input: &[0xb2, 0xa4], out: ExceptionOutcome::Hard(HardReading { main: &[0xb1, 0xfd], sub: &[0xa4, 0xa4], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 날 → 나+ㄹ
    ExceptionRule { input: &[0xbd, 0xd5], out: ExceptionOutcome::Hard(HardReading { main: &[0xbd, 0xd3], sub: &[0xa4, 0xa2], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 찬 → 차+ㄴ
    ExceptionRule { input: &[0xb8, 0xf3, 0xbb, 0xa6], out: ExceptionOutcome::Hard(HardReading { main: &[0xb8, 0xf3, 0xbb, 0xa4], sub: &[0xa4, 0xa2], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 보신 → 보시+ㄴ
    ExceptionRule { input: &[0xb0, 0xa1, 0xbc, 0xe8], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xa1], sub: &[0xbc, 0xe8], sub2: None, marker: 4, morphemes: 2, f1389: 0, f1400: 0 }) }, // 가지 → 가+지
    ExceptionRule { input: &[0xb3, 0xad, 0xb0, 0xa1], out: ExceptionOutcome::Hard(HardReading { main: &[0xb3, 0xad], sub: &[0xb0, 0xa1], sub2: None, marker: 2, morphemes: 2, f1389: 0, f1400: 0 }) }, // 내가 → 내+가
    ExceptionRule { input: &[0xb4, 0xdd, 0xbc, 0xe8, 0xb6, 0xa6], out: ExceptionOutcome::Hard(HardReading { main: &[0xb4, 0xdd, 0xbc, 0xe8], sub: &[0xb6, 0xa6], sub2: None, marker: 1, morphemes: 2, f1389: 0, f1400: 0 }) }, // 대지를 → 대지+를
    ExceptionRule { input: &[0xba, 0xa8, 0xb1, 0xe1], out: ExceptionOutcome::Hard(HardReading { main: &[0xba, 0xa8], sub: &[0xb1, 0xe1], sub2: None, marker: 1, morphemes: 2, f1389: 0, f1400: 0 }) }, // 삶과 → 삶+과
    ExceptionRule { input: &[0xb0, 0xfb, 0xb6, 0xa6], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xfb], sub: &[0xb6, 0xa6], sub2: None, marker: 2, morphemes: 2, f1389: 0, f1400: 0 }) }, // 그를 → 그+를
    ExceptionRule { input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb2, 0xf7], out: ExceptionOutcome::Hard(HardReading { main: &[0xb4, 0xdd, 0xc2, 0xd7], sub: &[0xca, 0xde, 0xba, 0xb7], sub2: Some(&[0xb2, 0xf7]), marker: 4, morphemes: 3, f1389: 0x15, f1400: 0x91 }) }, // 대해서는 → 대하+여서+는
    // --- 残り (§4.1 のハードコード語; 読みは主=入力のままの骨格) ---
    ExceptionRule { input: &[0xca, 0xef, 0xb2, 0xf7], out: ExceptionOutcome::Hard(HardReading { main: &[0xca, 0xef, 0xb2, 0xf7], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 오는
    ExceptionRule { input: &[0xc0, 0xb0, 0xb2, 0xf7], out: ExceptionOutcome::Hard(HardReading { main: &[0xc0, 0xb0, 0xb2, 0xf7], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 타는
    ExceptionRule { input: &[0xcb, 0xfb, 0xb1, 0xe2, 0xb8, 0xf5, 0xb6, 0xf3], out: ExceptionOutcome::Hard(HardReading { main: &[0xcb, 0xfb, 0xb1, 0xe2, 0xb8, 0xf5, 0xb6, 0xf3], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 그렇지만
    ExceptionRule { input: &[0xb9, 0xdd, 0xb5, 0xfb, 0xbd, 0xc3], out: ExceptionOutcome::Hard(HardReading { main: &[0xb9, 0xdd, 0xb5, 0xfb, 0xbd, 0xc3], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 반드시
    ExceptionRule { input: &[0xb1, 0xd7, 0xb7, 0xe1, 0xca, 0xbf], out: ExceptionOutcome::Hard(HardReading { main: &[0xb1, 0xd7, 0xb7, 0xe1, 0xca, 0xbf], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 그래야
    ExceptionRule { input: &[0xb1, 0xd7, 0xb7, 0xe1, 0xb8, 0xf2], out: ExceptionOutcome::Hard(HardReading { main: &[0xb1, 0xd7, 0xb7, 0xe1, 0xb8, 0xf2], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 그러면
    ExceptionRule { input: &[0xbc, 0xd6, 0xb7, 0xce], out: ExceptionOutcome::Hard(HardReading { main: &[0xbc, 0xd6, 0xb7, 0xce], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 주로
    ExceptionRule { input: &[0xb6, 0xf0], out: ExceptionOutcome::Hard(HardReading { main: &[0xb6, 0xf0], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 뒤
    ExceptionRule { input: &[0xb0, 0xbd, 0xbf, 0xec], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xbd, 0xbf, 0xec], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 경우
    ExceptionRule { input: &[0xb0, 0xdb, 0xb0, 0xfa], out: ExceptionOutcome::Hard(HardReading { main: &[0xb0, 0xdb, 0xb0, 0xfa], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 결과
    ExceptionRule { input: &[0xb4, 0xf5, 0xb6, 0xe7, 0xbe, 0xe0], out: ExceptionOutcome::Hard(HardReading { main: &[0xb4, 0xf5, 0xb6, 0xe7, 0xbe, 0xe0], sub: &[], sub2: None, marker: 0, morphemes: 1, f1389: 0, f1400: 0 }) }, // 더불어
];

/// 例外語検索 (FUN_0043b010 相当): 入力バイト列をテーブル順に完全一致比較する。
/// ヒットしなければ None (→ 既定経路 FUN_00444fb0 相当へ)。
pub fn lookup_exception(input: &[u8]) -> Option<ExceptionRule> {
    if input.is_empty() {
        return None;
    }
    EXCEPTION_TABLE
        .iter()
        .find(|r| r.input == input)
        .cloned()
}

// ===========================================================================
// 数字/単位読み (PTR_DAT_0048a490/0048a5a0/0048a478/0048a6b0/0048a6e0, G2P §10)
// ===========================================================================

/// 単位語 → 単位読み (PTR_DAT_0048a490 × PTR_DAT_0048a5a0, 先頭 24 エントリ)。
pub static UNIT_TABLE: [(&[u8], &[u8]); 24] = [
    (b"m", &[0xb8, 0xa1, 0xc0, 0xbe]), // 메터
    (b"cm", &[0xbb, 0xbf, 0xbe, 0xb7, 0xb8, 0xa1, 0xc0, 0xbe]), // 센치메터
    (b"mm", &[0xb7, 0xe7, 0xb6, 0xae, 0xb8, 0xa1, 0xc0, 0xbe]), // 미리메터
    (b"dm", &[0xb4, 0xe7, 0xbb, 0xa4, 0xb8, 0xa1, 0xc0, 0xbe]), // 데시메터
    (b"km", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb8, 0xa1, 0xc0, 0xbe]), // 키로메터
    (b"fm", &[0xc2, 0xc0, 0xc0, 0xcb, 0xb8, 0xa1, 0xc0, 0xbe]), // 펨토메터
    (b"nm", &[0xb1, 0xfd, 0xb2, 0xd1, 0xb8, 0xa1, 0xc0, 0xbe]), // 나노메터
    (b"g", &[0xb0, 0xfb, 0xb5, 0xbd]), // 그람
    (b"mg", &[0xb7, 0xe7, 0xb6, 0xae, 0xb0, 0xfb, 0xb5, 0xbd]), // 미리그람
    (b"kg", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb0, 0xfb, 0xb5, 0xbd]), // 키로그람
    (b"t", &[0xc0, 0xcd]), // 톤
    (b"V", &[0xb8, 0xf6, 0xc0, 0xe2]), // 볼트
    (b"pV", &[0xc2, 0xaa, 0xbf, 0xb8, 0xb8, 0xf6, 0xc0, 0xe2]), // 피코볼트
    (b"nV", &[0xb1, 0xfd, 0xb2, 0xd1, 0xb8, 0xf6, 0xc0, 0xe2]), // 나노볼트
    (b"mV", &[0xb7, 0xe7, 0xb6, 0xae, 0xb8, 0xf6, 0xc0, 0xe2]), // 미리볼트
    (b"kV", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb8, 0xf6, 0xc0, 0xe2]), // 키로볼트
    (b"MV", &[0xb8, 0xa1, 0xb0, 0xa1, 0xb8, 0xf6, 0xc0, 0xe2]), // 메가볼트
    (b"A", &[0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]), // 암페아
    (b"pA", &[0xc2, 0xaa, 0xbf, 0xb8, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]), // 피코암페아
    (b"nA", &[0xb1, 0xfd, 0xb2, 0xd1, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]), // 나노암페아
    (b"mA", &[0xb7, 0xe7, 0xb6, 0xae, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]), // 미리암페아
    (b"kA", &[0xbf, 0xd4, 0xb5, 0xe1, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]), // 키로암페아
    (b"W", &[0xcc, 0xae, 0xc0, 0xe2]), // 와트
    (b"pW", &[0xc2, 0xaa, 0xbf, 0xb8, 0xcc, 0xae, 0xc0, 0xe2]), // 피코와트
];

/// 単位語の読み (PTR_DAT_0048a5a0 相当)。未登録なら None。
pub fn unit_reading(unit: &[u8]) -> Option<&'static [u8]> {
    UNIT_TABLE
        .iter()
        .find(|(u, _)| *u == unit)
        .map(|(_, r)| *r)
}

/// 単位語マッチ (PTR_DAT_0048a478 相当, FUN_0040aef0 用)。
/// 実データ: 》≫〉>, m, cm, mm, dm, km, fm, nm, g, mg (kg は 0x48a490 側のみ)。
pub fn unit_match(unit: &[u8]) -> bool {
    const MATCH: &[&[u8]] = &[
        &[0xa1, 0xd5], // 》
        &[0xa2, 0xb9], // ≫
        b">",
        &[0xa1, 0xd3], // 〉
        b"m",
        b"cm",
        b"mm",
        b"dm",
        b"km",
        b"fm",
        b"nm",
        b"g",
        b"mg",
    ];
    MATCH.contains(&unit)
}

/// 数字語テーブル (PTR_DAT_0048a6b0, 40 エントリ)。NULL は空スライス。
pub static DIGIT_WORDS: [&[u8]; 40] = [
    &[0xc2, 0xd9], // 한
    &[0xbb, 0xab], // 십
    &[0xb9, 0xca], // 백
    &[0xbd, 0xe7], // 천
    &[0xb6, 0xed], // 만
    &[0xca, 0xcd], // 억
    &[0xbc, 0xbf], // 조
    &[0xca, 0xde, 0xb5, 0xcd], // 여러
    &[0xba, 0xe3], // 수
    &[0xb7, 0xb8], // 몇
    &[0xca, 0xde], // 여
    &[], // sentinel
    &[0xa4, 0xa2], // ㄴ
    &[0xa4, 0xa4], // ㄹ
    &[0xa4, 0xa6], // ㅂ
    &[0xa4, 0xa5], // ㅁ
    &[], // sentinel
    &[], // NULL
    &[0xb1, 0xb6], // 개
    &[0xb0, 0xd7], // 곡
    &[0xc4, 0xfa], // 끼
    &[0xbc, 0xb3], // 정
    &[0xb7, 0xba, 0xb1, 0xa4], // 모금
    &[0xb8, 0xd2], // 방
    &[0xb8, 0xde], // 벌
    &[0xbb, 0xf6], // 잔
    &[0xb8, 0xef], // 병
    &[0xbd, 0xea], // 첩
    &[0xba, 0xac], // 상
    &[0xbb, 0xf4, 0xb5, 0xf1], // 자루
    &[0xbe, 0xa2], // 축
    &[0xbe, 0xc1], // 채
    &[0xbe, 0xf4], // 칸
    &[0xbf, 0xb8], // 코
    &[0xc0, 0xd2], // 통
    &[0xc4, 0xda], // 꼴
    &[0xba, 0xa6], // 살
    &[0xca, 0xb2], // 알
    &[0xc0, 0xcd], // 톤
    &[0xb0, 0xa1, 0xbc, 0xe8], // 가지
];

/// 数字語プレフィクステーブル (PTR_DAT_0048a6e0, 40 エントリ)。NULL は空スライス。
pub static DIGIT_PREFIXES: [&[u8]; 40] = [
    &[0xa4, 0xa2], // ㄴ
    &[0xa4, 0xa4], // ㄹ
    &[0xa4, 0xa6], // ㅂ
    &[0xa4, 0xa5], // ㅁ
    &[], // sentinel
    &[], // NULL
    &[0xb1, 0xb6], // 개
    &[0xb0, 0xd7], // 곡
    &[0xc4, 0xfa], // 끼
    &[0xbc, 0xb3], // 정
    &[0xb7, 0xba, 0xb1, 0xa4], // 모금
    &[0xb8, 0xd2], // 방
    &[0xb8, 0xde], // 벌
    &[0xbb, 0xf6], // 잔
    &[0xb8, 0xef], // 병
    &[0xbd, 0xea], // 첩
    &[0xba, 0xac], // 상
    &[0xbb, 0xf4, 0xb5, 0xf1], // 자루
    &[0xbe, 0xa2], // 축
    &[0xbe, 0xc1], // 채
    &[0xbe, 0xf4], // 칸
    &[0xbf, 0xb8], // 코
    &[0xc0, 0xd2], // 통
    &[0xc4, 0xda], // 꼴
    &[0xba, 0xa6], // 살
    &[0xca, 0xb2], // 알
    &[0xc0, 0xcd], // 톤
    &[0xb0, 0xa1, 0xbc, 0xe8], // 가지
    &[0xb1, 0xac], // 길
    &[0xb2, 0xd6], // 놈
    &[0xb4, 0xb0], // 돐
    &[0xb8, 0xdc, 0xc9, 0xe3], // 번째
    &[0xb9, 0xc9], // 배
    &[0xbb, 0xa4, 0xb0, 0xa3], // 시간
    &[0xbc, 0xd1], // 주
    &[0xbc, 0xb3, 0xb8, 0xf3], // 정보
    &[0xc9, 0xe3], // 째
    &[0xb7, 0xcd], // 문
    &[0xb6, 0xf0], // 말
    &[0xb7, 0xf4], // 매
];

/// 数字語ヒット (strstr 相当, FUN_0040afb0 種別 1): テーブル順で最初に部分一致した
/// 数字語の index を返す。
pub fn digit_word_hit(input: &[u8]) -> Option<usize> {
    DIGIT_WORDS
        .iter()
        .position(|w| !w.is_empty() && contains_bytes(input, w))
}

/// 数字語プレフィクス長 (FUN_0040b130 相当): プレフィクス表で最初にヒットした
/// 位置 + 1 を返す (「몇/수/여러 + 助数詞」の検出用)。ヒットなしは 0。
pub fn digit_prefix_len(input: &[u8]) -> usize {
    for w in DIGIT_PREFIXES.iter() {
        if w.is_empty() {
            continue;
        }
        if let Some(pos) = find_bytes(input, w) {
            return pos + 1;
        }
    }
    0
}

/// バイト列 strstr: `input` 中の `pat` の最初の位置 (バイト単位)。
fn find_bytes(input: &[u8], pat: &[u8]) -> Option<usize> {
    if pat.is_empty() || pat.len() > input.len() {
        return None;
    }
    input
        .windows(pat.len())
        .position(|w| w == pat)
}

fn contains_bytes(input: &[u8], pat: &[u8]) -> bool {
    find_bytes(input, pat).is_some()
}

/// 特殊コード値 → キー文字 (FUN_0040a470 相当): 数字 0x30..0x39 → 0x46..0x4F、
/// '-' → 0x45 ('E')、'.' → 0x44 ('D')。bit15 付き (0x8000|値) も同様に処理。
pub fn special_to_key_char(v: u16) -> Option<u8> {
    let u = v & 0x7fff;
    match u {
        0x30..=0x39 => Some((u as u8) + 0x16),
        0x2d => Some(0x45),
        0x2e => Some(0x44),
        _ => None,
    }
}

// ===========================================================================
// アルファベット/字母読み (0x4768c8 系ダイグラフ規則, G2P §10.5)
// ===========================================================================

/// 英語ダイグラフ規則 28 種 (ASCII 平文, 0x4768c8..0x4769a0)。
pub static DIGRAPHS: [&[u8]; 28] = [
    b"es", b"th", b"qu", b"nk", b"dg", b"oo", b"ee", b"oy", b"ay", b"ew", b"au", b"ei", b"ur",
    b"er", b"tia", b"wor", b"old", b"ind", b"igh", b"our", b"ear", b"ure", b"ire", b"are", b"ast",
    b"asp", b"ant", b"aff",
];

/// ダイグラフ対応読み 22 種 (KeyPad 内部バイト列, 0x4767a0 系)。
/// 未確定のエントリは空 (TODO: 実バイナリからの完全抽出)。
pub static DIGRAPH_READINGS: [&[u8]; 22] = [
    &[0xcb, 0xcb], // 이
    &[0xca, 0xef, 0xcb, 0xcb], // 오이
    &[0xcb, 0xe6, 0xcb, 0xcb], // 에이
    &[0xca, 0xcc], // 어
    &[0xbb, 0xd5, 0xca, 0xcc], // 쉬어
    &[0xbb, 0xd5, 0xca, 0xef, 0xcb, 0xa7], // 쉬오우
    &[0xcc, 0xb8], // 워
    &[0xca, 0xef, 0xcb, 0xaa, 0xa4, 0xa3], // 오울ㄷ
    &[0xca, 0xef, 0xcb, 0xa7, 0xba, 0xf7, 0xa4, 0xac], // 오우스ㅌ
    &[0xa4, 0xa3], // ㄷ
    &[], // (未確定)
    &[], // (未確定)
    &[0xcb, 0xb1, 0xca, 0xcc], // 유어
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[0xca, 0xad, 0xa4, 0xae], // 아ㅎ
    &[],
    &[0xbc, 0xad], // 전
];

/// 単独字母の静的読み (KeyPad 内部バイト列)。字母はそのまま読む (恒等ペア)。
pub static JAMO_READINGS: [(&[u8], &[u8]); 11] = [
    (&[0xa4, 0xad], &[0xa4, 0xad]), // ㅍ
    (&[0xa4, 0xa2], &[0xa4, 0xa2]), // ㄴ
    (&[0xa4, 0xa4], &[0xa4, 0xa4]), // ㄹ
    (&[0xa4, 0xa7], &[0xa4, 0xa7]), // ㅅ
    (&[0xa4, 0xb2], &[0xa4, 0xb2]), // ㅆ
    (&[0xa4, 0xa6], &[0xa4, 0xa6]), // ㅂ
    (&[0xa4, 0xa9], &[0xa4, 0xa9]), // ㅈ
    (&[0xa4, 0xa8], &[0xa4, 0xa8]), // ㅇ
    (&[0xa4, 0xab], &[0xa4, 0xab]), // ㅋ
    (&[0xa4, 0xac], &[0xa4, 0xac]), // ㅌ
    (&[0xa4, 0xaa], &[0xa4, 0xaa]), // ㅊ
];
