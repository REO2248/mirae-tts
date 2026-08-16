//! WAV 出力 (FUN_0042b630 相当) — 46B ヘッダ + PCM。
//!
//! ヘッダはオリジナルの静的テーブル (0x48fbd8, 46B) + FUN_0042b630 の実行時パッチを
//! 再現したもの。テーブルは "RIF\0"/"fmt\0"/"dat\0" と文字を欠いた状態で格納され、
//! 書込直前にコードが 0x46('F') / 0x20(' ') / 0x61('a') をパッチする
//! (デコンパイル: `DAT_0048fbdb = 0x46; DAT_0048fbe7 = 0x20; DAT_0048fc03 = 0x61;`)。
//!
//! 最終ヘッダ (46B):
//! ```text
//! "RIFF" | u32(data_size + 0x30) | "WAVE" | "fmt " | u32(0x12) |
//! u16(1) | u16(1) | u32(22050) | u32(44100) | u16(2) | u16(16) | u16(0) |
//! "data" | u32(data_size)
//! ```
//! - fmt チャンクは 18B (WAVEFORMATEX 相当、cbSize=0 付き) — SPEC §2.7 の [0x10] は
//!   理想化記述で、実バイトは [0x12] (テーブル実測 + 3 分割 Write 0x14/0x12/8 で確定)。
//! - **RIFF サイズ = data_size + 0x30** (標準 36+data より +2 多い癖、差分 D13)。
//! - データは 46B 以降 (オープン直後に Seek(0x2e) でスキップ)、ヘッダは保存完了時に
//!   Seek(0) して後付け書込 (途中終了時はヘッダ無し)。
//! - 26MB 超でファイル分割 (同一名で再作成)。

use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// WAV ヘッダサイズ (46B = 0x2e)。
pub const WAV_HEADER_SIZE: usize = 46;

/// 分割しきい値 (累積 > 26,000,000B で FUN_0042b630 クローズ → 同名で再作成)。
pub const SPLIT_THRESHOLD_BYTES: u64 = 26_000_000;

/// サンプルレート (WAVEFORMATEX: 速度 50 × 441 = 22050Hz)。
pub const SAMPLE_RATE: u32 = 22050;
/// バイトレート (速度 50 × 882 = 44100B/s)。
pub const BYTE_RATE: u32 = 44100;

/// 46B ヘッダを `w` の先頭へ書き込む (FUN_0042b630 の書込部)。
///
/// 呼び出し前に Seek(0) する (オリジナルの `Seek(0,0,0)` → 3 分割 Write 相当)。
pub fn write_wav_header<W: Write + Seek>(w: &mut W, data_size: u32) -> io::Result<()> {
    w.seek(SeekFrom::Start(0))?;
    let mut h = [0u8; WAV_HEADER_SIZE];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(data_size + 0x30).to_le_bytes()); // RIFF = data+0x30 (癖)
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&0x12u32.to_le_bytes()); // fmt チャンクサイズ 18
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // wFormatTag = PCM
    h[22..24].copy_from_slice(&1u16.to_le_bytes()); // nChannels = 1
    h[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    h[28..32].copy_from_slice(&BYTE_RATE.to_le_bytes());
    h[32..34].copy_from_slice(&2u16.to_le_bytes()); // nBlockAlign
    h[34..36].copy_from_slice(&16u16.to_le_bytes()); // wBitsPerSample
    h[36..38].copy_from_slice(&0u16.to_le_bytes()); // WAVEFORMATEX cbSize = 0
    h[38..42].copy_from_slice(b"data");
    h[42..46].copy_from_slice(&data_size.to_le_bytes());
    w.write_all(&h)
}

/// 簡易 WAV 出力: ヘッダ 46B + PCM を 1 ファイルに書く。
///
/// 戻り値: PCM バイト数。
pub fn write_wav_file(path: &Path, pcm: &[u8]) -> io::Result<u64> {
    let mut w = WavWriter::create(path)?;
    w.append(pcm)?;
    w.finish()
}

/// ストリーミング WAV ライタ (FUN_0042b960 オープン + FUN_0042bd90 追記 + FUN_0042b630 クローズ相当)。
///
/// - `create`: ファイル作成後 `Seek(0x2e)` でヘッダ分スキップ (PCM は 46B 以降へ追記)
/// - `append`: PCM 追記。累積が 26MB 超でヘッダ書込+クローズ → 同名で再作成 (ファイル分割)
/// - `finish`: 先頭へ 46B ヘッダを書込んでクローズ
pub struct WavWriter {
    file: Option<File>,
    path: PathBuf,
    data_size: u64,
    split_threshold: u64,
}

impl WavWriter {
    /// 新規作成 (既存ファイルは上書き)。オープン直後にヘッダ 46B 分をスキップする。
    pub fn create(path: &Path) -> io::Result<Self> {
        Self::create_with_threshold(path, SPLIT_THRESHOLD_BYTES)
    }

    /// 分割しきい値を指定して作成 (テスト用)。
    pub fn create_with_threshold(path: &Path, split_threshold: u64) -> io::Result<Self> {
        let mut file = File::create(path)?;
        file.seek(SeekFrom::Start(WAV_HEADER_SIZE as u64))?;
        Ok(WavWriter {
            file: Some(file),
            path: path.to_path_buf(),
            data_size: 0,
            split_threshold,
        })
    }

    /// 現在の累積 PCM バイト数。
    pub fn data_size(&self) -> u64 {
        self.data_size
    }

    /// PCM を追記する (ヘッダはまだ書かない)。
    pub fn append(&mut self, pcm: &[u8]) -> io::Result<()> {
        let f = self.file.as_mut().expect("WavWriter already finished");
        f.write_all(pcm)?;
        self.data_size += pcm.len() as u64;
        if self.data_size > self.split_threshold {
            self.split()?;
        }
        Ok(())
    }

    /// 26MB 超のファイル分割: ヘッダ書込+クローズ → 同名で再作成 (データは 0 から)。
    fn split(&mut self) -> io::Result<()> {
        let f = self.file.as_mut().expect("WavWriter already finished");
        write_wav_header(f, self.data_size as u32)?;
        self.file = None; // Close
        let mut file = File::create(&self.path)?;
        file.seek(SeekFrom::Start(WAV_HEADER_SIZE as u64))?;
        self.file = Some(file);
        self.data_size = 0;
        Ok(())
    }

    /// 完了: 先頭へ 46B ヘッダを書込んでクローズ。戻り値は書き込んだ PCM バイト数。
    pub fn finish(mut self) -> io::Result<u64> {
        let f = self.file.as_mut().expect("WavWriter already finished");
        write_wav_header(f, self.data_size as u32)?;
        self.file = None; // Close
        Ok(self.data_size)
    }
}
