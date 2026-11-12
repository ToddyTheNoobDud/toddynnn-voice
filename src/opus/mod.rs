use napi::bindgen_prelude::*;
use napi::{Error, Result, Status};
use napi_derive::napi;

use opusic_sys::{
  self as sys, OPUS_APPLICATION_AUDIO, OPUS_APPLICATION_VOIP, OPUS_SET_BITRATE_REQUEST,
  OPUS_SET_COMPLEXITY_REQUEST, OPUS_SET_DTX_REQUEST, OPUS_SET_INBAND_FEC_REQUEST,
  OPUS_SET_PACKET_LOSS_PERC_REQUEST, OPUS_SET_VBR_REQUEST,
};
use std::{ffi::CStr, ptr::NonNull};

const MAX_PACKET_SIZE: usize = 4000;
const MAX_PACKET_DURATION_MS: usize = 120;

#[inline]
fn opus_fail(msg: impl Into<String>) -> Error {
  Error::new(Status::GenericFailure, msg.into())
}

#[inline]
fn invalid_arg(msg: impl Into<String>) -> Error {
  Error::new(Status::InvalidArg, msg.into())
}

fn validate_sample_rate(sample_rate: i32) -> Result<()> {
  if matches!(sample_rate, 8000 | 12000 | 16000 | 24000 | 48000) {
    Ok(())
  } else {
    Err(invalid_arg(format!(
      "Unsupported sampleRate {} (expected 8000/12000/16000/24000/48000)",
      sample_rate
    )))
  }
}

fn validate_frame_size(sample_rate_hz: i32, samples_per_channel: usize) -> Result<()> {
  let base = (sample_rate_hz as usize) / 400; // 2.5ms
  let allowed = [base, base * 2, base * 4, base * 8, base * 16, base * 24];
  if allowed.contains(&samples_per_channel) {
    Ok(())
  } else {
    Err(invalid_arg(format!(
      "Invalid frame size: {} samples/channel at {} Hz. Allowed: {:?}",
      samples_per_channel, sample_rate_hz, allowed
    )))
  }
}

fn parse_application(app: Option<Either<String, i32>>) -> Result<i32> {
  match app {
    None => Ok(OPUS_APPLICATION_AUDIO),

    Some(Either::B(n)) => {
      if matches!(n, OPUS_APPLICATION_AUDIO | OPUS_APPLICATION_VOIP | 2051) {
        Ok(n)
      } else {
        Err(invalid_arg(format!("Unsupported Opus application {}", n)))
      }
    }

    Some(Either::A(s)) => {
      if s.eq_ignore_ascii_case("audio") || s.eq_ignore_ascii_case("music") {
        Ok(OPUS_APPLICATION_AUDIO)
      } else if s.eq_ignore_ascii_case("voip") || s.eq_ignore_ascii_case("voice") {
        Ok(OPUS_APPLICATION_VOIP)
      } else if s.eq_ignore_ascii_case("lowdelay") {
        Ok(2051)
      } else {
        Err(invalid_arg(format!(
          "Unsupported application '{}'. Use 'audio' or 'voip'.",
          s
        )))
      }
    }
  }
}

fn pcm16_from_node_bytes<'a>(bytes: &'a [u8], scratch: &'a mut [i16]) -> &'a [i16] {
  #[cfg(target_endian = "little")]
  {
    let ptr = bytes.as_ptr() as usize;
    if ptr.is_multiple_of(std::mem::align_of::<i16>()) {
      let len = bytes.len() / 2;
      // SAFETY: PCM16 is aligned, initialized, and borrowed for this synchronous call.
      let s = unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const i16, len) };
      return s;
    }
  }

  let pcm = &mut scratch[..bytes.len() / 2];
  for (sample, chunk) in pcm.iter_mut().zip(bytes.as_chunks::<2>().0) {
    *sample = i16::from_le_bytes(*chunk);
  }
  pcm
}

#[napi(js_name = "getOpusVersion")]
pub fn get_opus_version() -> String {
  unsafe { CStr::from_ptr(sys::opus_get_version_string()) }
    .to_string_lossy()
    .into_owned()
}

#[napi(js_name = "opus_get_version_string")]
pub fn opus_get_version_string_js() -> String {
  get_opus_version()
}

#[napi]
pub struct OpusEncoder {
  enc: EncoderState,
  dec: DecoderState,
  sample_rate: i32,
  channels: i32,
  application: i32,

  scratch_pcm: Vec<i16>,
  scratch_decode: Vec<i16>,
}

#[napi]
impl OpusEncoder {
  #[napi(constructor)]
  pub fn new(
    sample_rate: i32,
    channels: i32,
    application: Option<Either<String, i32>>,
  ) -> Result<Self> {
    Self::create(sample_rate, channels, application)
  }

  #[napi(factory)]
  pub fn create(
    sample_rate: i32,
    channels: i32,
    application: Option<Either<String, i32>>,
  ) -> Result<Self> {
    validate_sample_rate(sample_rate)?;
    if channels != 1 && channels != 2 {
      return Err(invalid_arg(format!(
        "Unsupported channels {} (must be 1 or 2)",
        channels
      )));
    }

    let app = parse_application(application)?;

    let mut error = 0;
    let enc = unsafe { sys::opus_encoder_create(sample_rate, channels, app, &mut error) };
    let enc = EncoderState(
      NonNull::new(enc).ok_or_else(|| opus_fail(format!("Encoder creation failed: {error}")))?,
    );
    let dec = unsafe { sys::opus_decoder_create(sample_rate, channels, &mut error) };
    let dec = DecoderState(
      NonNull::new(dec).ok_or_else(|| opus_fail(format!("Decoder creation failed: {error}")))?,
    );

    let ch = channels as usize;
    let scratch_pcm = vec![0; (sample_rate as usize * 60 / 1000) * ch];

    let sr = sample_rate as usize;
    let max_samples_per_channel = (sr * MAX_PACKET_DURATION_MS) / 1000;
    let scratch_decode = vec![0; max_samples_per_channel * ch];

    let mut codec = Self {
      enc,
      dec,
      sample_rate,
      channels,
      application: app,
      scratch_pcm,
      scratch_decode,
    };
    for (request, value) in [
      (sys::OPUS_SET_VBR_REQUEST, 1),
      (sys::OPUS_SET_VBR_CONSTRAINT_REQUEST, 0),
      (sys::OPUS_SET_COMPLEXITY_REQUEST, 10),
      (sys::OPUS_SET_SIGNAL_REQUEST, sys::OPUS_SIGNAL_MUSIC),
      (sys::OPUS_SET_DTX_REQUEST, 0),
      (sys::OPUS_SET_INBAND_FEC_REQUEST, 0),
      (sys::OPUS_SET_PACKET_LOSS_PERC_REQUEST, 0),
      (
        sys::OPUS_SET_MAX_BANDWIDTH_REQUEST,
        sys::OPUS_BANDWIDTH_FULLBAND,
      ),
      (sys::OPUS_SET_LSB_DEPTH_REQUEST, 16),
      (
        sys::OPUS_SET_BITRATE_REQUEST,
        if channels == 2 { 128_000 } else { 96_000 },
      ),
    ] {
      codec.apply_encoder_ctl(request, value)?;
    }
    Ok(codec)
  }

  /// @public
  /// Encodes one PCM16LE frame into caller-owned memory.
  /// @param pcmBytes - One interleaved PCM16LE frame.
  /// @param out - Destination; 4000 bytes supports the wrapper's maximum packet budget.
  /// @returns Number of bytes written.
  /// Input and output must not overlap or be accessed concurrently.
  #[napi(js_name = "encodeInto", catch_unwind)]
  pub fn encode_into(
    &mut self,
    pcm_bytes: Uint8ArraySlice<'_>,
    mut out: Uint8ArraySlice<'_>,
  ) -> Result<u32> {
    reject_overlap(pcm_bytes.as_ref(), out.as_ref())?;
    // SAFETY: This synchronous call does not invoke JS; overlap was rejected above.
    self
      .encode_pcm(pcm_bytes.as_ref(), unsafe { out.as_mut() })
      .map(|written| written as u32)
  }

  #[napi(catch_unwind)]
  pub fn encode(&mut self, pcm_bytes: Uint8ArraySlice<'_>) -> Result<Buffer> {
    let mut out = vec![0; MAX_PACKET_SIZE];
    let written = self.encode_pcm(pcm_bytes.as_ref(), &mut out)?;
    out.truncate(written);
    Ok(out.into())
  }

  /// @public
  /// Decodes into caller-owned PCM16LE memory; aligned little-endian output is direct.
  /// @param packet - One Opus packet; an empty packet requests 120 ms concealment.
  /// @param out - Destination; 23040 bytes holds 120 ms at 48 kHz stereo.
  /// @returns Number of bytes written.
  /// Input and output must not overlap or be accessed concurrently.
  #[napi(js_name = "decodeInto", catch_unwind)]
  pub fn decode_into(
    &mut self,
    packet: Uint8ArraySlice<'_>,
    mut out: Uint8ArraySlice<'_>,
  ) -> Result<u32> {
    reject_overlap(packet.as_ref(), out.as_ref())?;
    let samples = packet_samples(packet.as_ref(), self.sample_rate)?;
    // SAFETY: This synchronous call does not invoke JS; overlap was rejected above.
    self
      .decode_pcm(packet.as_ref(), unsafe { out.as_mut() }, samples)
      .map(|written| written as u32)
  }

  #[napi(catch_unwind)]
  pub fn decode(&mut self, packet: Uint8ArraySlice<'_>) -> Result<Buffer> {
    let samples = packet_samples(packet.as_ref(), self.sample_rate)?;
    let mut out = vec![0; samples * self.channels as usize * 2];
    let written = self.decode_pcm(packet.as_ref(), &mut out, samples)?;
    out.truncate(written);
    Ok(out.into())
  }

  #[napi(js_name = "setBitrate", catch_unwind)]
  pub fn set_bitrate(&mut self, bitrate: i32) -> Result<()> {
    if bitrate <= 0 {
      return Err(invalid_arg(format!("bitrate must be > 0, got {}", bitrate)));
    }
    self.apply_encoder_ctl(OPUS_SET_BITRATE_REQUEST, bitrate)
  }

  #[napi(js_name = "setDTX", catch_unwind)]
  pub fn set_dtx(&mut self, enabled: bool) -> Result<()> {
    self.apply_encoder_ctl(OPUS_SET_DTX_REQUEST, i32::from(enabled))
  }

  #[napi(js_name = "setFEC", catch_unwind)]
  pub fn set_fec(&mut self, enabled: bool) -> Result<()> {
    self.apply_encoder_ctl(OPUS_SET_INBAND_FEC_REQUEST, i32::from(enabled))
  }

  #[napi(js_name = "setPLP", catch_unwind)]
  pub fn set_plp(&mut self, percent: i32) -> Result<()> {
    if !(0..=100).contains(&percent) {
      return Err(invalid_arg(format!("PLP must be 0..100, got {}", percent)));
    }
    self.apply_encoder_ctl(OPUS_SET_PACKET_LOSS_PERC_REQUEST, percent)
  }

  #[napi(js_name = "setVBR", catch_unwind)]
  pub fn set_vbr(&mut self, enabled: bool) -> Result<()> {
    self.apply_encoder_ctl(OPUS_SET_VBR_REQUEST, i32::from(enabled))
  }

  #[napi(js_name = "setConstrainedVBR", catch_unwind)]
  pub fn set_constrained_vbr(&mut self, enabled: bool) -> Result<()> {
    self.apply_encoder_ctl(sys::OPUS_SET_VBR_CONSTRAINT_REQUEST, i32::from(enabled))
  }

  #[napi(js_name = "setComplexity", catch_unwind)]
  pub fn set_complexity(&mut self, complexity: i32) -> Result<()> {
    self.apply_encoder_ctl(OPUS_SET_COMPLEXITY_REQUEST, complexity.clamp(0, 10))
  }

  #[napi(js_name = "setSignal", catch_unwind)]
  pub fn set_signal(&mut self, kind: String) -> Result<()> {
    let signal = if kind.eq_ignore_ascii_case("music") || kind.eq_ignore_ascii_case("audio") {
      sys::OPUS_SIGNAL_MUSIC
    } else if kind.eq_ignore_ascii_case("voice") || kind.eq_ignore_ascii_case("voip") {
      sys::OPUS_SIGNAL_VOICE
    } else {
      return Err(invalid_arg("signal must be music or voice"));
    };
    self.apply_encoder_ctl(sys::OPUS_SET_SIGNAL_REQUEST, signal)
  }

  #[napi(js_name = "applyEncoderCTL", catch_unwind)]
  pub fn apply_encoder_ctl(&mut self, request: i32, value: i32) -> Result<()> {
    let result = if request == sys::OPUS_RESET_STATE {
      unsafe { sys::opus_encoder_ctl(self.enc.0.as_ptr(), request) }
    } else {
      validate_integer_ctl(request)?;
      unsafe { sys::opus_encoder_ctl(self.enc.0.as_ptr(), request, value) }
    };
    check_opus(result).map(|_| ())
  }

  #[napi(js_name = "applyEncoderCtl", catch_unwind)]
  pub fn apply_encoder_ctl_alias(&mut self, request: i32, value: i32) -> Result<()> {
    self.apply_encoder_ctl(request, value)
  }

  #[napi(js_name = "encoderCTL", catch_unwind)]
  pub fn encoder_ctl_alias(&mut self, request: i32, value: i32) -> Result<()> {
    self.apply_encoder_ctl(request, value)
  }

  #[napi(js_name = "applyDecoderCTL", catch_unwind)]
  pub fn apply_decoder_ctl(&mut self, request: i32, value: i32) -> Result<()> {
    let result = if request == sys::OPUS_RESET_STATE {
      unsafe { sys::opus_decoder_ctl(self.dec.0.as_ptr(), request) }
    } else {
      validate_integer_ctl(request)?;
      unsafe { sys::opus_decoder_ctl(self.dec.0.as_ptr(), request, value) }
    };
    check_opus(result).map(|_| ())
  }

  #[napi(js_name = "getVersion")]
  pub fn get_version(&self) -> String {
    get_opus_version()
  }

  #[napi(getter, js_name = "sampleRate")]
  pub fn sample_rate(&self) -> i32 {
    self.sample_rate
  }

  #[napi(getter, js_name = "channels")]
  pub fn channels(&self) -> i32 {
    self.channels
  }

  #[napi(getter, js_name = "application")]
  pub fn application(&self) -> i32 {
    self.application
  }
}

impl OpusEncoder {
  fn encode_pcm(&mut self, bytes: &[u8], output: &mut [u8]) -> Result<usize> {
    let bytes_per_sample_frame = self.channels as usize * 2;
    if !bytes.len().is_multiple_of(bytes_per_sample_frame) {
      return Err(invalid_arg(
        "PCM16LE byte length must be divisible by channels * 2",
      ));
    }
    let frame_size = bytes.len() / bytes_per_sample_frame;
    validate_frame_size(self.sample_rate, frame_size)?;
    let pcm = pcm16_from_node_bytes(bytes, &mut self.scratch_pcm);
    let capacity =
      i32::try_from(output.len()).map_err(|_| invalid_arg("Output buffer too large"))?;
    let result = unsafe {
      sys::opus_encode(
        self.enc.0.as_ptr(),
        pcm.as_ptr(),
        frame_size as i32,
        output.as_mut_ptr(),
        capacity,
      )
    };
    check_opus(result)
  }

  fn decode_pcm(&mut self, packet: &[u8], output: &mut [u8], samples: usize) -> Result<usize> {
    let sample_count = samples * self.channels as usize;
    let bytes = sample_count * 2;
    if output.len() < bytes {
      return Err(invalid_arg("Output buffer too small for packet"));
    }
    let direct = cfg!(target_endian = "little")
      && (output.as_ptr() as usize).is_multiple_of(std::mem::align_of::<i16>());
    let destination = if direct {
      output.as_mut_ptr().cast::<i16>()
    } else {
      self.scratch_decode.as_mut_ptr()
    };
    /* INFO: packet_samples checked this length before the private decode call. */
    let packet_length = packet.len() as i32;
    let result = unsafe {
      sys::opus_decode(
        self.dec.0.as_ptr(),
        if packet.is_empty() {
          std::ptr::null()
        } else {
          packet.as_ptr()
        },
        packet_length,
        destination,
        samples as i32,
        0,
      )
    };
    let decoded = check_opus(result)? * self.channels as usize;
    if !direct {
      for (sample, bytes) in self.scratch_decode[..decoded]
        .iter()
        .zip(output.as_chunks_mut::<2>().0)
      {
        *bytes = sample.to_le_bytes();
      }
    }
    Ok(decoded * 2)
  }
}

/* INFO: Each owner frees its codec on constructor failure and normal destruction. */
struct EncoderState(NonNull<sys::OpusEncoder>);
struct DecoderState(NonNull<sys::OpusDecoder>);

impl Drop for EncoderState {
  fn drop(&mut self) {
    unsafe { sys::opus_encoder_destroy(self.0.as_ptr()) }
  }
}
impl Drop for DecoderState {
  fn drop(&mut self) {
    unsafe { sys::opus_decoder_destroy(self.0.as_ptr()) }
  }
}

fn check_opus(result: i32) -> Result<usize> {
  if result < 0 {
    Err(opus_fail(format!("libopus failed with code {result}")))
  } else {
    Ok(result as usize)
  }
}

fn validate_integer_ctl(request: i32) -> Result<()> {
  /* INFO: Getters and blob controls require pointers; admit integer setters only.
  Keep this boundary explicit when updating the libopus headers. */
  if matches!(
    request,
    sys::OPUS_SET_APPLICATION_REQUEST
      | sys::OPUS_SET_BITRATE_REQUEST
      | sys::OPUS_SET_MAX_BANDWIDTH_REQUEST
      | sys::OPUS_SET_VBR_REQUEST
      | sys::OPUS_SET_BANDWIDTH_REQUEST
      | sys::OPUS_SET_COMPLEXITY_REQUEST
      | sys::OPUS_SET_INBAND_FEC_REQUEST
      | sys::OPUS_SET_PACKET_LOSS_PERC_REQUEST
      | sys::OPUS_SET_DTX_REQUEST
      | sys::OPUS_SET_VBR_CONSTRAINT_REQUEST
      | sys::OPUS_SET_FORCE_CHANNELS_REQUEST
      | sys::OPUS_SET_SIGNAL_REQUEST
      | sys::OPUS_SET_GAIN_REQUEST
      | sys::OPUS_SET_LSB_DEPTH_REQUEST
      | sys::OPUS_SET_EXPERT_FRAME_DURATION_REQUEST
      | sys::OPUS_SET_PREDICTION_DISABLED_REQUEST
      | sys::OPUS_SET_PHASE_INVERSION_DISABLED_REQUEST
      | sys::OPUS_SET_DRED_DURATION_REQUEST
      | sys::OPUS_SET_OSCE_BWE_REQUEST
      | sys::OPUS_SET_QEXT_REQUEST
      | sys::OPUS_SET_IGNORE_EXTENSIONS_REQUEST
  ) {
    Ok(())
  } else {
    Err(invalid_arg(
      "CTL must be a supported integer setter request",
    ))
  }
}

fn reject_overlap(input: &[u8], output: &[u8]) -> Result<()> {
  let input_start = input.as_ptr() as usize;
  let output_start = output.as_ptr() as usize;
  if input_start < output_start + output.len() && output_start < input_start + input.len() {
    return Err(invalid_arg("Input and output buffers must not overlap"));
  }
  Ok(())
}

/* INFO: libopus validates packet geometry before output access or decoder mutation. */
fn packet_samples(packet: &[u8], sample_rate: i32) -> Result<usize> {
  if packet.is_empty() {
    return Ok(sample_rate as usize * MAX_PACKET_DURATION_MS / 1000);
  }
  let length = i32::try_from(packet.len()).map_err(|_| invalid_arg("Packet too large"))?;
  check_opus(unsafe { sys::opus_packet_get_nb_samples(packet.as_ptr(), length, sample_rate) })
}
