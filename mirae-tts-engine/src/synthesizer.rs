//! Mirae pipeline: KPS segment → phoneme (+Speech/COLLIGATION) → unit select → VoiceData PCM.

use std::borrow::Cow;
use std::io;
use std::path::{Path, PathBuf};

use crate::colligation::Colligation;
use crate::english::english_to_korean;
use crate::number::apply_number_conversion;
use crate::phoneme::{text_to_phonemes_with_context, PhonemeUnit};
use crate::segmenter::{segment, BreakType, SegKind};
use crate::speech::SpeechDict;
use crate::unit_select::{select_units_for_sequence, smooth_pitch_pass};
use crate::voice_info::{VoiceDataReader, VoiceInfo};
use crate::wave_render;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub sample_rate: u32,
    pub sentence_pause: i16,
    /// Progress and warnings via `tracing`. Default `false` for library embeds; enable for CLI-style feedback.
    pub log_progress: bool,
}

impl Default for TtsConfig {
    fn default() -> Self {
        TtsConfig {
            sample_rate: 22050,
            sentence_pause: 4000,
            log_progress: false,
        }
    }
}

pub struct TtsEngine {
    voice_info: VoiceInfo,
    voice_data: VoiceDataReader,
    config: TtsConfig,
    #[allow(dead_code)]
    voice_dir: PathBuf,
    speech_dict: Option<SpeechDict>,
    #[allow(dead_code)]
    colligation: Option<Colligation>,
}

impl TtsEngine {
    /// VoiceInfo.pkg + VoiceData.pkg required; optional Speech.pkg, COLLIGATION.PKG.
    pub fn new<P: AsRef<Path>>(voice_dir: P, config: TtsConfig) -> io::Result<Self> {
        let voice_dir = voice_dir.as_ref().to_path_buf();
        let log = config.log_progress;

        let voice_info_path = voice_dir.join("VoiceInfo.pkg");
        if !voice_info_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("VoiceInfo.pkg not found in {:?}", voice_dir),
            ));
        }
        let voice_info = VoiceInfo::load(&voice_info_path)?;

        let voice_data_path = voice_dir.join("VoiceData.pkg");
        if !voice_data_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("VoiceData.pkg not found in {:?}", voice_dir),
            ));
        }
        let voice_data = VoiceDataReader::open(&voice_data_path)?;

        if log {
            debug!(
                "[TtsEngine] Initialized with {} voice entries",
                voice_info.entries.len()
            );
        }

        let speech_dict = {
            let p = voice_dir.join("Speech.pkg");
            if p.exists() {
                match SpeechDict::load(&p) {
                    Ok(d) => {
                        if log {
                            debug!("[TtsEngine] Loaded Speech.pkg ({} entries)", d.len());
                        }
                        Some(d)
                    }
                    Err(e) => {
                        if log {
                            warn!("[TtsEngine] Warning: could not load Speech.pkg: {e}");
                        }
                        None
                    }
                }
            } else {
                None
            }
        };

        let colligation = {
            let p = voice_dir.join("COLLIGATION.PKG");
            if p.exists() {
                match Colligation::load(&p) {
                    Ok(c) => {
                        if log {
                            debug!(
                                "[TtsEngine] Loaded COLLIGATION.PKG ({} nodes, {} rules)",
                                c.node_count(),
                                c.record_count()
                            );
                        }
                        Some(c)
                    }
                    Err(e) => {
                        if log {
                            warn!("[TtsEngine] Warning: could not load COLLIGATION.PKG: {e}");
                        }
                        None
                    }
                }
            } else {
                None
            }
        };

        Ok(TtsEngine {
            voice_info,
            voice_data,
            config,
            voice_dir,
            speech_dict,
            colligation,
        })
    }

    pub fn synthesize(&self, text: &str) -> io::Result<Vec<i16>> {
        let replaced_text = Self::apply_text_replacements(text);
        let phonemes = self.text_to_phoneme_sequence(&replaced_text);

        if phonemes.is_empty() {
            if self.config.log_progress {
                    debug!("[TtsEngine] No phonemes generated for input text");
            }
            return Ok(Vec::new());
        }

        if self.config.log_progress {
                debug!("[TtsEngine] Generated {} phoneme units", phonemes.len());
        }

        // Type-1 template hypos before vowel split / CV fallback.
        let type1_sids: Vec<u16> = phonemes
            .iter()
            .filter(|p| p.pause.is_none() && p.syllable_id != 0xFFFF)
            .map(|p| p.syllable_id)
            .collect();
        let type1_matches = if self.colligation.is_some() {
            crate::colligation::find_type1_matches(&type1_sids)
        } else {
            Vec::new()
        };

        let mut selected = select_units_for_sequence(&self.voice_info, &phonemes, &type1_matches);

        smooth_pitch_pass(&self.voice_info, &phonemes, &mut selected, 15); // Hz outlier threshold

        let matched_count = selected.iter().filter(|s| s.is_some()).count();
        if self.config.log_progress {
                debug!(
                "[TtsEngine] Selected {}/{} voice units",
                matched_count,
                phonemes.len()
            );
        }

        if matched_count == 0 {
            if self.config.log_progress {
                warn!(
                    "[TtsEngine] Warning: No voice units matched. Check VoiceInfo.pkg format."
                );
            }
            return Ok(Vec::new());
        }

        let pcm = wave_render::render_to_pcm(
            &self.voice_data,
            &phonemes,
            &selected,
            self.config.sentence_pause,
        )?;

        if self.config.log_progress {
                debug!("[TtsEngine] Rendered {} PCM samples", pcm.len());
        }

        Ok(pcm)
    }

    /// Per-segment PCM (lower latency than `synthesize`); `&self` + `Arc` = parallel-safe.
    pub fn synthesize_streaming<F>(&self, text: &str, mut on_chunk: F) -> io::Result<()>
    where
        F: FnMut(Vec<i16>) -> bool,
    {
        let kps_bytes = crate::kps9566_encode(text);
        let segments = segment(&kps_bytes);
        if segments.is_empty() {
            return Ok(());
        }

        let mut prev_col: i8 = 4; // phrase-start row

        let seg_count = segments.len();
        for (si, seg) in segments.iter().enumerate() {
            let seg_utf8 = crate::kps9566_decode(seg.bytes);

            let pronunciation: Cow<str> = if seg.kind == SegKind::Korean {
                if let Some(dict) = &self.speech_dict {
                    dict.lookup(&seg_utf8)
                        .map(Cow::Borrowed)
                        .unwrap_or_else(|| Cow::Borrowed(&seg_utf8))
                } else {
                    Cow::Borrowed(&seg_utf8)
                }
            } else if seg.kind == SegKind::Latin {
                Cow::Owned(english_to_korean(&seg_utf8))
            } else if seg.kind == SegKind::Number {
                Cow::Owned(apply_number_conversion(&seg_utf8, false))
            } else {
                Cow::Borrowed(&seg_utf8)
            };

            let trailing_break = match seg.break_after {
                BreakType::Clause => Some(BreakType::Clause),
                BreakType::Sentence => Some(BreakType::Sentence),
                BreakType::None => None,
            };

            let mut seg_units: Vec<PhonemeUnit> = Vec::new();
            let more = si + 1 < seg_count;
            let last_col = text_to_phonemes_with_context(
                &pronunciation,
                prev_col,
                trailing_break,
                seg.after_whitespace,
                more,
                &mut seg_units,
            );
            prev_col = last_col;

            if seg_units.is_empty() {
                continue;
            }

            if let Some(ref coll) = self.colligation {
                let sids: Vec<u16> = seg_units.iter().map(|u| u.syllable_id).collect();
                let marks = coll.apply_type45_rules(&sids);
                for (unit, &marked) in seg_units.iter_mut().zip(marks.iter()) {
                    unit.colligation_variant = marked;
                }
            }

            let type1_sids: Vec<u16> = seg_units
                .iter()
                .filter(|p| p.pause.is_none() && p.syllable_id != 0xFFFF)
                .map(|p| p.syllable_id)
                .collect();
            let type1_matches = if self.colligation.is_some() {
                crate::colligation::find_type1_matches(&type1_sids)
            } else {
                Vec::new()
            };

            let mut selected =
                select_units_for_sequence(&self.voice_info, &seg_units, &type1_matches);
            smooth_pitch_pass(&self.voice_info, &seg_units, &mut selected, 15); // Hz

            let pcm = wave_render::render_to_pcm(
                &self.voice_data,
                &seg_units,
                &selected,
                self.config.sentence_pause,
            )?;

            if pcm.is_empty() {
                continue;
            }

            if !on_chunk(pcm) {
                break;
            }
        }

        Ok(())
    }

    /// Mirae private-use placeholders → hangul (same as original resource strings).
    fn apply_text_replacements(text: &str) -> String {
        let mut result = text.to_string();

        let replacements: &[(&str, &str)] = &[
            ("", "김"),
            ("", "일"),
            ("", "성"),
            ("", "김"),
            ("", "정"),
            ("", "일"),
            ("", "김"),
            ("", "정"),
            ("", "은"),
        ];

        for (from, to) in replacements {
            result = result.replace(from, to);
        }

        result
    }

    fn text_to_phoneme_sequence(&self, text: &str) -> Vec<PhonemeUnit> {
        let kps_bytes = crate::kps9566_encode(text);

        let segments = segment(&kps_bytes);

        if segments.is_empty() {
            return Vec::new();
        }

        let mut all_units: Vec<PhonemeUnit> = Vec::with_capacity(segments.len() * 8);
        let mut prev_col: i8 = 4; // phrase-start row

        let seg_count = segments.len();
        for (si, seg) in segments.iter().enumerate() {
            let seg_utf8 = crate::kps9566_decode(seg.bytes);

            let pronunciation: Cow<str> = if seg.kind == SegKind::Korean {
                if let Some(dict) = &self.speech_dict {
                    dict.lookup(&seg_utf8)
                        .map(Cow::Borrowed)
                        .unwrap_or_else(|| Cow::Borrowed(&seg_utf8))
                } else {
                    Cow::Borrowed(&seg_utf8)
                }
            } else if seg.kind == SegKind::Latin {
                Cow::Owned(english_to_korean(&seg_utf8))
            } else if seg.kind == SegKind::Number {
                Cow::Owned(apply_number_conversion(&seg_utf8, false))
            } else {
                Cow::Borrowed(&seg_utf8)
            };

            let trailing_break = match seg.break_after {
                BreakType::Clause => Some(BreakType::Clause),
                BreakType::Sentence => Some(BreakType::Sentence),
                BreakType::None => None,
            };

            let more = si + 1 < seg_count;
            let last_col = text_to_phonemes_with_context(
                &pronunciation,
                prev_col,
                trailing_break,
                seg.after_whitespace,
                more,
                &mut all_units,
            );

            prev_col = last_col;
        }

        if let Some(ref coll) = self.colligation {
            let sids: Vec<u16> = all_units.iter().map(|u| u.syllable_id).collect();
            let marks = coll.apply_type45_rules(&sids);
            for (unit, &marked) in all_units.iter_mut().zip(marks.iter()) {
                unit.colligation_variant = marked;
            }
        }

        all_units
    }

    pub fn effective_sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    pub fn voice_entry_count(&self) -> usize {
        self.voice_info.entries.len()
    }

    pub fn config(&self) -> &TtsConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: TtsConfig) {
        self.config = config;
    }
}
