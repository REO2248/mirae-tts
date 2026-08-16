//! VoiceData.pkg — ユニット波形読み出し (FUN_0044b700 相当)
//!
//! オリジナルはユニット毎に `CFile::Open(path, modeRead)` → `Seek(woff×2, current)` →
//! `Read(wlen×2)` → `Close` を繰り返す (571MB のコーパスを都度オープン)。
//! Open 直後のファイル位置は 0 なので `Seek(..., SEEK_CUR)` は実質絶対シークであり、
//! 本実装ではハンドルを開きっぱなしにして絶対シークで等価の読み出しを行う
//! (SPEC §1.2: 「ファイルを開きっぱなしにする実装でもよい」)。
//!
//! オフセット計算は「サンプル × 2 = バイト」を厳守すること (woff/wlen はサンプル単位)。
//!
//! 読み込み先は 64,000B スクラッチバッファ (FUN_0044c2e0 が `FUN_00429d80(&local_10, 64000)`
//! で確保)。ユニット長の上限 (フィルタ wlen_max=12000 → 24,000B) はこれを超えない。

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// 読み込み先スクラッチバッファサイズ (オリジナル: 64,000B = 0xfa00)。
pub const SCRATCH_SIZE: usize = 64000;

/// VoiceData.pkg への読み出しハンドル。
///
/// オリジナルの FUN_0044b700 と同じセマンティクス:
/// `read_unit(woff, wlen)` はファイル位置を `woff×2` へ移動して `wlen×2` バイトを読み、
/// 呼び出し後もハンドルは開いたまま (次回の絶対シークで位置を合わせる)。
pub struct VoiceData {
    file: File,
    path: PathBuf,
}

impl VoiceData {
    /// `<dir>/VoiceData.pkg` を開く。
    pub fn open(dir: &Path) -> io::Result<Self> {
        Self::open_path(&dir.join("VoiceData.pkg"))
    }

    /// パスを直接指定して開く。
    pub fn open_path(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(VoiceData {
            file,
            path: path.to_path_buf(),
        })
    }

    /// 開いているファイルのパス。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// ファイルサイズ (バイト)。
    pub fn len(&self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    /// FUN_0044b700 相当: `Seek(woff×2) → Read(wlen×2)`。
    ///
    /// `out` は少なくとも `wlen×2` バイト必要。戻り値は読み込んだバイト数 (= wlen×2)。
    /// ユニット長の上限チェック: 64,000B スクラッチを超える要求は panic (オリジナルは
    /// 64KB バッファに書き込むため同じ制約)。
    pub fn read_unit(&mut self, woff: u32, wlen: u32, out: &mut [u8]) -> io::Result<usize> {
        let byte_off = u64::from(woff) * 2;
        let byte_len = usize::try_from(u64::from(wlen) * 2).expect("wlen×2 overflow");
        assert!(
            byte_len <= SCRATCH_SIZE,
            "unit read {}B exceeds 64,000B scratch buffer (wlen={})",
            byte_len,
            wlen
        );
        assert!(
            out.len() >= byte_len,
            "output buffer {}B too small for {}B unit",
            out.len(),
            byte_len
        );
        self.file.seek(SeekFrom::Start(byte_off))?;
        self.file.read_exact(&mut out[..byte_len])?;
        Ok(byte_len)
    }

    /// 1 ユニットを `Vec<u8>` として読み出す (PCM バイト列、s16le)。
    pub fn read_unit_vec(&mut self, woff: u32, wlen: u32) -> io::Result<Vec<u8>> {
        let byte_len = usize::try_from(u64::from(wlen) * 2).expect("wlen×2 overflow");
        let mut buf = vec![0u8; byte_len];
        self.read_unit(woff, wlen, &mut buf)?;
        Ok(buf)
    }

    /// 1 ユニットを `Vec<i16>` (サンプル列) として読み出す。
    pub fn read_unit_samples(&mut self, woff: u32, wlen: u32) -> io::Result<Vec<i16>> {
        let bytes = self.read_unit_vec(woff, wlen)?;
        let mut samples = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
        }
        Ok(samples)
    }
}
