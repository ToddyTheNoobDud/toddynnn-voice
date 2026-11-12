#![allow(non_camel_case_types)]

use napi::bindgen_prelude::*;
use napi::{Result, Task};
use symphonia::core::{
  codecs::audio::{CODEC_ID_NULL_AUDIO, well_known::*},
  formats::{FormatOptions, probe::Hint},
  io::MediaSourceStream,
  meta::{MetadataOptions, MetadataRevision},
};
use symphonia::default::get_probe;

#[napi(object, js_name = "ProbeResult")]
pub struct JsProbeResult {
  /// The number of channels
  pub channels: u32,
  /// The sample rate
  pub sample_rate: u32,
  /// The number of frames per block
  pub frames_per_block: u32,
  /// The codec type
  #[napi(ts_type = "CodecType")]
  pub codec: JsCodecType,
  /// The number of frames
  pub n_frames: u32,
  /// The approximate duration of this media in seconds
  pub duration: u32,
  /// The metadata object
  pub metadata: Vec<MetadataField>,
}

#[napi(js_name = "CodecType")]
pub enum JsCodecType {
  // Unknown
  UNKNOWN = 0,
  // PCM
  PCM_S32LE = 0x100,
  PCM_S32LE_PLANAR = 0x101,
  PCM_S32BE = 0x102,
  PCM_S32BE_PLANAR = 0x103,
  PCM_S24LE = 0x104,
  PCM_S24LE_PLANAR = 0x105,
  PCM_S24BE = 0x106,
  PCM_S24BE_PLANAR = 0x107,
  PCM_S16LE = 0x108,
  PCM_S16LE_PLANAR = 0x109,
  PCM_S16BE = 0x10a,
  PCM_S16BE_PLANAR = 0x10b,
  PCM_S8 = 0x10c,
  PCM_S8_PLANAR = 0x10d,
  PCM_U32LE = 0x10e,
  PCM_U32LE_PLANAR = 0x10f,
  PCM_U32BE = 0x110,
  PCM_U32BE_PLANAR = 0x111,
  PCM_U24LE = 0x112,
  PCM_U24LE_PLANAR = 0x113,
  PCM_U24BE = 0x114,
  PCM_U24BE_PLANAR = 0x115,
  PCM_U16LE = 0x116,
  PCM_U16LE_PLANAR = 0x117,
  PCM_U16BE = 0x118,
  PCM_U16BE_PLANAR = 0x119,
  PCM_U8 = 0x11a,
  PCM_U8_PLANAR = 0x11b,
  PCM_F32LE = 0x11c,
  PCM_F32LE_PLANAR = 0x11d,
  PCM_F32BE = 0x11e,
  PCM_F32BE_PLANAR = 0x11f,
  PCM_F64LE = 0x120,
  PCM_F64LE_PLANAR = 0x121,
  PCM_F64BE = 0x122,
  PCM_F64BE_PLANAR = 0x123,
  PCM_ALAW = 0x124,
  PCM_MULAW = 0x125,
  // ADPCM
  ADPCM_G722 = 0x200,
  ADPCM_G726 = 0x201,
  ADPCM_G726LE = 0x202,
  ADPCM_MS = 0x203,
  ADPCM_IMA_WAV = 0x204,
  ADPCM_IMA_QT = 0x205,
  // Vorbis
  VORBIS = 0x1000,
  // MPEG Layer 1 (MP1)
  MP1 = 0x1001,
  // MPEG Layer 2 (MP2)
  MP2 = 0x1002,
  // MPEG Layer 3 (MP3)
  MP3 = 0x1003,
  // Advanced Audio Coding (AAC)
  AAC = 0x1004,
  // Opus
  OPUS = 0x1005,
  // Speex
  SPEEX = 0x1006,
  // Musepack
  MUSEPACK = 0x1007,
  // Adaptive Transform Acoustic Coding (ATRAC1)
  ATRAC1 = 0x1008,
  // Adaptive Transform Acoustic Coding 3 (ATRAC3)
  ATRAC3 = 0x1009,
  // Adaptive Transform Acoustic Coding 3+ (ATRAC3+)
  ATRAC3PLUS = 0x100a,
  // Adaptive Transform Acoustic Coding 9 (ATRAC9)
  ATRAC9 = 0x100b,
  // AC-3, E-AC-3, Dolby Digital (ATSC A/52)
  EAC3 = 0x100c,
  // Dolby AC-4 (ETSI TS 103 190)
  AC4 = 0x100d,
  // DTS Coherent Acoustics (DCA/DTS)
  DCA = 0x100e,
  // Windows Media Audio
  WMA = 0x100f,
  // FLAC
  FLAC = 0x2000,
  // WavPack
  WAVPACK = 0x2001,
  // Monkey's Audio (APE)
  MONKEYS_AUDIO = 0x2002,
  // Apple Lossless Audio Codec (ALAC)
  ALAC = 0x2003,
  // True Audio (TTA)
  TTA = 0x2004,
}

#[napi(object)]
pub struct MetadataField {
  pub name: String,
  pub value: String,
}

#[napi(catch_unwind, ts_return_type = "Promise<ProbeResult>")]
pub fn probe(data: BufferSlice<'_>) -> AsyncTask<ProbeTask> {
  /* INFO: Snapshot on the JS thread; the caller may mutate its buffer after return. */
  AsyncTask::new(ProbeTask {
    input: data.to_vec(),
  })
}

#[napi(catch_unwind)]
pub fn probe_sync(data: BufferSlice<'_>) -> Result<JsProbeResult> {
  probe_inner(data.as_ref())
}

pub struct ProbeTask {
  input: Vec<u8>,
}

impl Task for ProbeTask {
  type Output = JsProbeResult;
  type JsValue = JsProbeResult;

  fn compute(&mut self) -> Result<Self::Output> {
    probe_inner(&self.input)
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(output)
  }
}

fn probe_inner(data: &[u8]) -> Result<JsProbeResult> {
  let cursor = std::io::Cursor::new(data);

  let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

  let hint = Hint::new();

  let meta_opts: MetadataOptions = Default::default();
  let fmt_opts: FormatOptions = Default::default();

  let mut format = get_probe()
    .probe(&hint, mss, fmt_opts, meta_opts)
    .map_err(|e| {
      Error::new(
        Status::GenericFailure,
        format!("failed to probe media: {}", e),
      )
    })?;

  let metadata = format
    .metadata()
    .current()
    .map(parse_metadata)
    .unwrap_or_default();

  let track = format
    .tracks()
    .iter()
    .find(|track| {
      track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .is_some_and(|params| params.codec != CODEC_ID_NULL_AUDIO)
    })
    .ok_or(Error::new(Status::GenericFailure, "no audio tracks found"))?;

  let codec_params = track
    .codec_params
    .as_ref()
    .and_then(|params| params.audio())
    .expect("audio parameters selected above");

  Ok(JsProbeResult {
    channels: codec_params
      .channels
      .as_ref()
      .map_or(1, |channels| channels.count()) as u32,
    sample_rate: codec_params.sample_rate.unwrap_or(44100),
    frames_per_block: codec_params.frames_per_block.unwrap_or(0) as u32,
    codec: match codec_params.codec {
      CODEC_ID_PCM_S32LE => JsCodecType::PCM_S32LE,
      CODEC_ID_PCM_S32LE_PLANAR => JsCodecType::PCM_S32LE_PLANAR,
      CODEC_ID_PCM_S32BE => JsCodecType::PCM_S32BE,
      CODEC_ID_PCM_S32BE_PLANAR => JsCodecType::PCM_S32BE_PLANAR,
      CODEC_ID_PCM_S24LE => JsCodecType::PCM_S24LE,
      CODEC_ID_PCM_S24LE_PLANAR => JsCodecType::PCM_S24LE_PLANAR,
      CODEC_ID_PCM_S24BE => JsCodecType::PCM_S24BE,
      CODEC_ID_PCM_S24BE_PLANAR => JsCodecType::PCM_S24BE_PLANAR,
      CODEC_ID_PCM_S16LE => JsCodecType::PCM_S16LE,
      CODEC_ID_PCM_S16LE_PLANAR => JsCodecType::PCM_S16LE_PLANAR,
      CODEC_ID_PCM_S16BE => JsCodecType::PCM_S16BE,
      CODEC_ID_PCM_S16BE_PLANAR => JsCodecType::PCM_S16BE_PLANAR,
      CODEC_ID_PCM_S8 => JsCodecType::PCM_S8,
      CODEC_ID_PCM_S8_PLANAR => JsCodecType::PCM_S8_PLANAR,
      CODEC_ID_PCM_U32LE => JsCodecType::PCM_U32LE,
      CODEC_ID_PCM_U32LE_PLANAR => JsCodecType::PCM_U32LE_PLANAR,
      CODEC_ID_PCM_U32BE => JsCodecType::PCM_U32BE,
      CODEC_ID_PCM_U32BE_PLANAR => JsCodecType::PCM_U32BE_PLANAR,
      CODEC_ID_PCM_U24LE => JsCodecType::PCM_U24LE,
      CODEC_ID_PCM_U24LE_PLANAR => JsCodecType::PCM_U24LE_PLANAR,
      CODEC_ID_PCM_U24BE => JsCodecType::PCM_U24BE,
      CODEC_ID_PCM_U24BE_PLANAR => JsCodecType::PCM_U24BE_PLANAR,
      CODEC_ID_PCM_U16LE => JsCodecType::PCM_U16LE,
      CODEC_ID_PCM_U16LE_PLANAR => JsCodecType::PCM_U16LE_PLANAR,
      CODEC_ID_PCM_U16BE => JsCodecType::PCM_U16BE,
      CODEC_ID_PCM_U16BE_PLANAR => JsCodecType::PCM_U16BE_PLANAR,
      CODEC_ID_PCM_U8 => JsCodecType::PCM_U8,
      CODEC_ID_PCM_U8_PLANAR => JsCodecType::PCM_U8_PLANAR,
      CODEC_ID_PCM_F32LE => JsCodecType::PCM_F32LE,
      CODEC_ID_PCM_F32LE_PLANAR => JsCodecType::PCM_F32LE_PLANAR,
      CODEC_ID_PCM_F32BE => JsCodecType::PCM_F32BE,
      CODEC_ID_PCM_F32BE_PLANAR => JsCodecType::PCM_F32BE_PLANAR,
      CODEC_ID_PCM_F64LE => JsCodecType::PCM_F64LE,
      CODEC_ID_PCM_F64LE_PLANAR => JsCodecType::PCM_F64LE_PLANAR,
      CODEC_ID_PCM_F64BE => JsCodecType::PCM_F64BE,
      CODEC_ID_PCM_F64BE_PLANAR => JsCodecType::PCM_F64BE_PLANAR,
      CODEC_ID_PCM_ALAW => JsCodecType::PCM_ALAW,
      CODEC_ID_PCM_MULAW => JsCodecType::PCM_MULAW,
      CODEC_ID_ADPCM_G722 => JsCodecType::ADPCM_G722,
      CODEC_ID_ADPCM_G726 => JsCodecType::ADPCM_G726,
      CODEC_ID_ADPCM_G726LE => JsCodecType::ADPCM_G726LE,
      CODEC_ID_ADPCM_MS => JsCodecType::ADPCM_MS,
      CODEC_ID_ADPCM_IMA_WAV => JsCodecType::ADPCM_IMA_WAV,
      CODEC_ID_ADPCM_IMA_QT => JsCodecType::ADPCM_IMA_QT,
      CODEC_ID_VORBIS => JsCodecType::VORBIS,
      CODEC_ID_MP1 => JsCodecType::MP1,
      CODEC_ID_MP2 => JsCodecType::MP2,
      CODEC_ID_MP3 => JsCodecType::MP3,
      CODEC_ID_AAC => JsCodecType::AAC,
      CODEC_ID_OPUS => JsCodecType::OPUS,
      CODEC_ID_SPEEX => JsCodecType::SPEEX,
      CODEC_ID_MUSEPACK => JsCodecType::MUSEPACK,
      CODEC_ID_ATRAC1 => JsCodecType::ATRAC1,
      CODEC_ID_ATRAC3 => JsCodecType::ATRAC3,
      CODEC_ID_ATRAC3PLUS => JsCodecType::ATRAC3PLUS,
      CODEC_ID_ATRAC9 => JsCodecType::ATRAC9,
      CODEC_ID_EAC3 => JsCodecType::EAC3,
      CODEC_ID_AC4 => JsCodecType::AC4,
      CODEC_ID_DCA => JsCodecType::DCA,
      CODEC_ID_WMA => JsCodecType::WMA,
      CODEC_ID_FLAC => JsCodecType::FLAC,
      CODEC_ID_WAVPACK => JsCodecType::WAVPACK,
      CODEC_ID_MONKEYS_AUDIO => JsCodecType::MONKEYS_AUDIO,
      CODEC_ID_ALAC => JsCodecType::ALAC,
      CODEC_ID_TTA => JsCodecType::TTA,
      _ => JsCodecType::UNKNOWN,
    },
    n_frames: track.num_frames.unwrap_or(0) as u32,
    duration: track
      .time_base
      .zip(track.duration)
      .and_then(|(time_base, duration)| time_base.calc_duration(duration))
      .map_or(0, |time| time.as_secs().max(0) as u32),
    metadata,
  })
}

fn parse_metadata(rev: &MetadataRevision) -> Vec<MetadataField> {
  let tags = &rev.media.tags;
  tags
    .iter()
    .map(|tag| MetadataField {
      name: tag.raw.key.to_string(),
      value: tag.raw.value.to_string(),
    })
    .collect()
}
