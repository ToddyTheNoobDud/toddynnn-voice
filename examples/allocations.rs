use opusic_sys as sys;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    if COUNTING.load(Ordering::Relaxed) {
      ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
    unsafe { System.alloc(layout) }
  }

  unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
    unsafe { System.dealloc(pointer, layout) }
  }

  unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
    if COUNTING.load(Ordering::Relaxed) {
      ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
    unsafe { System.realloc(pointer, layout, size) }
  }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
  let mut error = 0;
  let encoder =
    unsafe { sys::opus_encoder_create(48000, 2, sys::OPUS_APPLICATION_AUDIO, &mut error) };
  assert!(!encoder.is_null());
  let decoder = unsafe { sys::opus_decoder_create(48000, 2, &mut error) };
  assert!(!decoder.is_null());
  let mut input = [0i16; 1920];
  let mut packet = [0u8; 4000];
  let mut output = [0i16; 1920];
  for (index, sample) in input.iter_mut().enumerate() {
    *sample = ((index as f32 * 0.073).sin() * 12000.0) as i16;
  }
  // Warm up lazy codec tables before counting, as a live player does.
  for _ in 0..100 {
    let written = unsafe {
      sys::opus_encode(
        encoder,
        input.as_ptr(),
        960,
        packet.as_mut_ptr(),
        packet.len() as i32,
      )
    };
    assert!(written > 0);
    let decoded = unsafe {
      sys::opus_decode(
        decoder,
        packet.as_ptr(),
        written,
        output.as_mut_ptr(),
        960,
        0,
      )
    };
    assert_eq!(decoded, 960);
  }
  COUNTING.store(true, Ordering::Relaxed);
  for _ in 0..1000 {
    let written = unsafe {
      sys::opus_encode(
        encoder,
        input.as_ptr(),
        960,
        packet.as_mut_ptr(),
        packet.len() as i32,
      )
    };
    assert!(written > 0);
    let decoded = unsafe {
      sys::opus_decode(
        decoder,
        packet.as_ptr(),
        written,
        output.as_mut_ptr(),
        960,
        0,
      )
    };
    assert_eq!(decoded, 960);
  }
  COUNTING.store(false, Ordering::Relaxed);
  let allocations = ALLOCATIONS.load(Ordering::Relaxed);
  println!(
    "Rust allocator: {allocations} allocations/reallocations across 1000 stereo encode+decode pairs"
  );
  assert_eq!(allocations, 0);
  unsafe {
    sys::opus_encoder_destroy(encoder);
    sys::opus_decoder_destroy(decoder);
  }
  println!("This counter covers Rust allocations only, not libopus's C allocator");
}
