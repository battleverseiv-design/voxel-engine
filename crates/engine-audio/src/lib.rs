//! Advanced Modular Audio Engine for Voxel Engine.
//! Provides:
//! - Multi-channel software mixer with zero external dependencies (uses native Windows waveOut).
//! - 3D Spatial Audio (distance attenuation, stereo panning relative to listener camera).
//! - Data-driven SoundRegistry with variations, pitch jitter, and category volume control.
//! - Seamless ambient audio system with automatic smooth crossfading.
//! - Non-blocking concurrent playback (voices never cut each other off).

#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundCategory {
    Master = 0,
    Sfx = 1,
    Ambient = 2,
    Music = 3,
    Ui = 4,
}

/// In-memory audio buffer of PCM samples normalized to [-1.0, 1.0].
#[derive(Clone)]
pub struct SoundBuffer {
    /// Interleaved audio samples: for mono `[s0, s1, ...]`, for stereo `[l0, r0, l1, r1, ...]`.
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

impl SoundBuffer {
    /// Loads a standard 16-bit or 8-bit PCM WAV file.
    pub fn from_wav_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err("Invalid WAV RIFF header".to_string());
        }

        let mut offset = 12;
        let mut channels = 1u16;
        let mut sample_rate = 22050u32;
        let mut bits_per_sample = 16u16;
        let mut audio_data: Option<&[u8]> = None;

        while offset + 8 <= bytes.len() {
            let chunk_id = &bytes[offset..offset + 4];
            let chunk_size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            offset += 8;

            if offset + chunk_size > bytes.len() {
                break;
            }

            if chunk_id == b"fmt " && chunk_size >= 16 {
                let format_tag = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
                if format_tag != 1 {
                    return Err("Unsupported non-PCM WAV format".to_string());
                }
                channels = u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().unwrap());
                sample_rate = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
                bits_per_sample = u16::from_le_bytes(bytes[offset + 14..offset + 16].try_into().unwrap());
            } else if chunk_id == b"data" {
                audio_data = Some(&bytes[offset..offset + chunk_size]);
            }

            offset += chunk_size;
            if chunk_size % 2 == 1 {
                offset += 1;
            }
        }

        let raw_data = audio_data.ok_or_else(|| "WAV missing data chunk".to_string())?;
        let mut samples = Vec::new();

        if bits_per_sample == 16 {
            let sample_count = raw_data.len() / 2;
            samples.reserve(sample_count);
            for i in 0..sample_count {
                let val = i16::from_le_bytes([raw_data[i * 2], raw_data[i * 2 + 1]]);
                samples.push(val as f32 / 32768.0);
            }
        } else if bits_per_sample == 8 {
            samples.reserve(raw_data.len());
            for &b in raw_data {
                let norm = (b as f32 - 128.0) / 128.0;
                samples.push(norm);
            }
        } else {
            return Err(format!("Unsupported bit depth: {bits_per_sample}"));
        }

        Ok(Self {
            samples,
            channels,
            sample_rate,
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let file = File::open(path.as_ref()).map_err(|e| format!("Failed to open {}: {}", path.as_ref().display(), e))?;
        let mut reader = BufReader::new(file);
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        Self::from_wav_bytes(&bytes)
    }

    #[inline]
    pub fn get_stereo_sample(&self, frame: usize) -> (f32, f32) {
        if self.channels == 1 {
            let s = self.samples.get(frame).copied().unwrap_or(0.0);
            (s, s)
        } else {
            let idx = frame * 2;
            let l = self.samples.get(idx).copied().unwrap_or(0.0);
            let r = self.samples.get(idx + 1).copied().unwrap_or(0.0);
            (l, r)
        }
    }

    #[inline]
    pub fn sample_linear(&self, cursor: f64) -> (f32, f32) {
        let f0 = cursor.floor() as usize;
        let frac = (cursor - f0 as f64) as f32;
        let (l0, r0) = self.get_stereo_sample(f0);
        let (l1, r1) = self.get_stereo_sample(f0 + 1);
        (l0 + frac * (l1 - l0), r0 + frac * (r1 - r0))
    }

    #[inline]
    pub fn total_frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / (self.channels as usize)
        }
    }
}

/// 3D listener position and orientation.
#[derive(Debug, Clone, Copy)]
pub struct Listener {
    pub position: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            right: Vec3::X,
        }
    }
}

/// Individual active playing voice in the mixer.
struct Voice {
    buffer: Arc<SoundBuffer>,
    category: SoundCategory,
    cursor: f64,
    pitch: f64,
    volume: f32,
    position: Option<Vec3>,
    max_distance: f32,
    looping: bool,
    fade_multiplier: f32,
    fade_delta: f32, // for crossfades
    finished: bool,
}

/// Definition of a sound event in the registry.
#[derive(Clone)]
pub struct SoundEventDef {
    pub category: SoundCategory,
    pub variations: Vec<Arc<SoundBuffer>>,
    pub volume: f32,
    pub max_distance: f32,
    pub pitch_jitter: f32,
}

impl SoundEventDef {
    pub fn new(category: SoundCategory, variations: Vec<Arc<SoundBuffer>>) -> Self {
        Self {
            category,
            variations,
            volume: 1.0,
            max_distance: 32.0,
            pitch_jitter: 0.04,
        }
    }

    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    pub fn with_distance(mut self, max_distance: f32) -> Self {
        self.max_distance = max_distance;
        self
    }

    pub fn with_pitch_jitter(mut self, jitter: f32) -> Self {
        self.pitch_jitter = jitter;
        self
    }
}

struct MixerState {
    listener: Listener,
    volumes: [f32; 5],
    voices: Vec<Voice>,
    current_ambient_name: Option<String>,
    registry: HashMap<String, SoundEventDef>,
    sample_cache: HashMap<String, Arc<SoundBuffer>>,
    rng_state: u32,
}

impl MixerState {
    fn next_rand_f32(&mut self) -> f32 {
        self.rng_state = self.rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.rng_state >> 16) as f32 / 65536.0
    }
}

// ---------------------------------------------------------------------------
// Native Windows waveOut FFI
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
struct WAVEFORMATEX {
    w_format_tag: u16,
    n_channels: u16,
    n_samples_per_sec: u32,
    n_avg_bytes_per_sec: u32,
    n_block_align: u16,
    w_bits_per_sample: u16,
    cb_size: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WAVEHDR {
    lp_data: *mut u8,
    dw_buffer_length: u32,
    dw_bytes_recorded: u32,
    dw_user: usize,
    dw_flags: u32,
    dw_loops: u32,
    lp_next: *mut core::ffi::c_void,
    reserved: usize,
}

const WHDR_DONE: u32 = 0x00000001;
const WHDR_PREPARED: u32 = 0x00000002;
const MIX_SAMPLE_RATE: u32 = 44100;
const BUFFER_SAMPLES: usize = 1024; // ~23.2 ms latency with rock-solid stability
const NUM_BUFFERS: usize = 4;

#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(u_period: u32) -> u32;
    fn timeEndPeriod(u_period: u32) -> u32;

    fn waveOutOpen(
        phwo: *mut *mut core::ffi::c_void,
        u_device_id: u32,
        pwfx: *const WAVEFORMATEX,
        dw_callback: usize,
        dw_instance: usize,
        fdw_open: u32,
    ) -> u32;

    fn waveOutPrepareHeader(hwo: *mut core::ffi::c_void, pwh: *mut WAVEHDR, cbwh: u32) -> u32;
    fn waveOutUnprepareHeader(hwo: *mut core::ffi::c_void, pwh: *mut WAVEHDR, cbwh: u32) -> u32;
    fn waveOutWrite(hwo: *mut core::ffi::c_void, pwh: *mut WAVEHDR, cbwh: u32) -> u32;
    fn waveOutClose(hwo: *mut core::ffi::c_void) -> u32;
    fn waveOutReset(hwo: *mut core::ffi::c_void) -> u32;
}

// ---------------------------------------------------------------------------
// AudioEngine Controller
// ---------------------------------------------------------------------------

pub struct AudioEngine {
    state: Arc<Mutex<MixerState>>,
    running: Arc<AtomicBool>,
    _worker_thread: Option<JoinHandle<()>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let initial_state = MixerState {
            listener: Listener::default(),
            volumes: [1.0, 1.0, 0.75, 0.8, 1.0], // Master, Sfx, Ambient, Music, Ui
            voices: Vec::with_capacity(64),
            current_ambient_name: None,
            registry: HashMap::new(),
            sample_cache: HashMap::new(),
            rng_state: 77777,
        };

        let state = Arc::new(Mutex::new(initial_state));
        let running = Arc::new(AtomicBool::new(true));

        let state_worker = Arc::clone(&state);
        let running_worker = Arc::clone(&running);

        let worker = thread::Builder::new()
            .name("AudioMixerThread".to_string())
            .spawn(move || {
                run_audio_worker(state_worker, running_worker);
            })
            .ok();

        Self {
            state,
            running,
            _worker_thread: worker,
        }
    }

    /// Sets the position, forward and right vectors of the listener camera.
    pub fn set_listener(&self, pos: Vec3, forward: Vec3, right: Vec3) {
        if let Ok(mut lock) = self.state.lock() {
            lock.listener = Listener {
                position: pos,
                forward: forward.normalize_or_zero(),
                right: right.normalize_or_zero(),
            };
        }
    }

    /// Sets the volume for a specific category (0.0 to 1.0).
    pub fn set_volume(&self, category: SoundCategory, vol: f32) {
        if let Ok(mut lock) = self.state.lock() {
            lock.volumes[category as usize] = vol.clamp(0.0, 2.0);
        }
    }

    /// Loads and caches a sound buffer from disk.
    pub fn load_sound(&self, path_str: &str) -> Option<Arc<SoundBuffer>> {
        let mut lock = self.state.lock().ok()?;
        if let Some(buf) = lock.sample_cache.get(path_str) {
            return Some(Arc::clone(buf));
        }

        // Try direct path, relative assets/sounds/, or path next to current exe
        let mut paths = vec![
            path_str.to_string(),
            format!("assets/sounds/{path_str}"),
            format!("../assets/sounds/{path_str}"),
        ];

        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                paths.push(parent.join("assets/sounds").join(path_str).to_string_lossy().to_string());
                paths.push(parent.join(path_str).to_string_lossy().to_string());
            }
        }

        for p in &paths {
            if let Ok(buf) = SoundBuffer::from_file(p) {
                let arc = Arc::new(buf);
                lock.sample_cache.insert(path_str.to_string(), Arc::clone(&arc));
                return Some(arc);
            }
        }

        None
    }

    /// Registers a sound event definition with name (e.g. `"block.break.stone"`).
    pub fn register_event(&self, name: impl Into<String>, def: SoundEventDef) {
        if let Ok(mut lock) = self.state.lock() {
            lock.registry.insert(name.into(), def);
        }
    }

    /// Helper to register a sound event with file paths directly.
    pub fn register_simple_event(
        &self,
        name: &str,
        category: SoundCategory,
        files: &[&str],
        volume: f32,
        max_dist: f32,
    ) {
        let mut variations = Vec::new();
        for &f in files {
            if let Some(buf) = self.load_sound(f) {
                variations.push(buf);
            }
        }
        if !variations.is_empty() {
            let def = SoundEventDef::new(category, variations)
                .with_volume(volume)
                .with_distance(max_dist);
            self.register_event(name, def);
        }
    }

    /// Plays a registered sound event by name with optional 3D world position.
    pub fn play_event(&self, name: &str, pos: Option<Vec3>) {
        if let Ok(mut lock) = self.state.lock() {
            let def = match lock.registry.get(name) {
                Some(d) => d.clone(),
                None => return,
            };
            if def.variations.is_empty() {
                return;
            }

            let rnd = lock.next_rand_f32();
            let idx = (rnd * def.variations.len() as f32).floor() as usize % def.variations.len();
            let buffer = Arc::clone(&def.variations[idx]);

            let pitch_jitter = (lock.next_rand_f32() - 0.5) * 2.0 * def.pitch_jitter;
            let pitch = (1.0 + pitch_jitter as f64).clamp(0.5, 2.0);

            let voice = Voice {
                buffer,
                category: def.category,
                cursor: 0.0,
                pitch,
                volume: def.volume,
                position: pos,
                max_distance: def.max_distance,
                looping: false,
                fade_multiplier: 1.0,
                fade_delta: 0.0,
                finished: false,
            };

            lock.voices.push(voice);
        }
    }

    /// Plays a direct sound buffer in 2D or 3D.
    pub fn play_sound(
        &self,
        buffer: Arc<SoundBuffer>,
        category: SoundCategory,
        pos: Option<Vec3>,
        volume: f32,
    ) {
        if let Ok(mut lock) = self.state.lock() {
            let voice = Voice {
                buffer,
                category,
                cursor: 0.0,
                pitch: 1.0,
                volume,
                position: pos,
                max_distance: 32.0,
                looping: false,
                fade_multiplier: 1.0,
                fade_delta: 0.0,
                finished: false,
            };
            lock.voices.push(voice);
        }
    }

    /// Plays a 2D interface sound effect.
    pub fn play_ui(&self, name: &str) {
        self.play_event(name, None);
    }

    /// Switches the active background ambient loop with smooth crossfading.
    pub fn set_ambient(&self, name: &str) {
        if let Ok(mut lock) = self.state.lock() {
            if lock.current_ambient_name.as_deref() == Some(name) {
                return;
            }
            lock.current_ambient_name = Some(name.to_string());

            // Start fading out existing ambient voices
            for v in lock.voices.iter_mut() {
                if v.category == SoundCategory::Ambient && v.looping {
                    v.looping = false; // stop infinite loop
                    v.fade_delta = -1.0 / (MIX_SAMPLE_RATE as f32 * 1.5); // 1.5 sec fade out
                }
            }

            // Start new ambient voice
            let def = match lock.registry.get(name) {
                Some(d) => d.clone(),
                None => return,
            };
            if let Some(buf) = def.variations.first() {
                let voice = Voice {
                    buffer: Arc::clone(buf),
                    category: SoundCategory::Ambient,
                    cursor: 0.0,
                    pitch: 1.0,
                    volume: def.volume,
                    position: None, // 2D ambient wrap
                    max_distance: 100.0,
                    looping: true,
                    fade_multiplier: 0.0, // starts at 0 and fades in
                    fade_delta: 1.0 / (MIX_SAMPLE_RATE as f32 * 1.5), // 1.5 sec fade in
                    finished: false,
                };
                lock.voices.push(voice);
            }
        }
    }

    /// Stops any running ambient background audio with fade out.
    pub fn stop_ambient(&self) {
        if let Ok(mut lock) = self.state.lock() {
            lock.current_ambient_name = None;
            for v in lock.voices.iter_mut() {
                if v.category == SoundCategory::Ambient {
                    v.looping = false;
                    v.fade_delta = -1.0 / (MIX_SAMPLE_RATE as f32 * 1.0);
                }
            }
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Realtime Audio Worker Thread
// ---------------------------------------------------------------------------

fn run_audio_worker(state: Arc<Mutex<MixerState>>, running: Arc<AtomicBool>) {
    let mut h_wave_out: *mut core::ffi::c_void = core::ptr::null_mut();

    let wfx = WAVEFORMATEX {
        w_format_tag: 1, // PCM
        n_channels: 2,   // Stereo
        n_samples_per_sec: MIX_SAMPLE_RATE,
        n_avg_bytes_per_sec: MIX_SAMPLE_RATE * 2 * 2, // 176400
        n_block_align: 4,                             // 2 channels * 2 bytes
        w_bits_per_sample: 16,                        // 16 bit
        cb_size: 0,
    };

    let open_res = unsafe {
        waveOutOpen(
            &mut h_wave_out,
            0xFFFFFFFF, // WAVE_MAPPER
            &wfx,
            0,
            0,
            0,
        )
    };

    if open_res != 0 || h_wave_out.is_null() {
        eprintln!("[engine-audio] Failed to open waveOut device: {open_res}");
        return;
    }

    // Set Windows scheduler timer resolution to 1ms for rock-solid real-time audio
    unsafe { timeBeginPeriod(1); }

    // Allocate cyclic output buffers (interleaved i16 stereo)
    let buffer_bytes = BUFFER_SAMPLES * 2 * 2; // 2 channels * 2 bytes = 4096 bytes
    let mut raw_buffers = vec![vec![0u8; buffer_bytes]; NUM_BUFFERS];
    let mut headers = vec![
        WAVEHDR {
            lp_data: core::ptr::null_mut(),
            dw_buffer_length: buffer_bytes as u32,
            dw_bytes_recorded: 0,
            dw_user: 0,
            dw_flags: 0,
            dw_loops: 0,
            lp_next: core::ptr::null_mut(),
            reserved: 0,
        };
        NUM_BUFFERS
    ];

    for i in 0..NUM_BUFFERS {
        headers[i].lp_data = raw_buffers[i].as_mut_ptr();
        unsafe {
            waveOutPrepareHeader(h_wave_out, &mut headers[i], std::mem::size_of::<WAVEHDR>() as u32);
            headers[i].dw_flags |= WHDR_DONE; // Mark ready initially
        }
    }

    let mut current_buf = 0;
    let mut mix_left = [0.0f32; BUFFER_SAMPLES];
    let mut mix_right = [0.0f32; BUFFER_SAMPLES];

    while running.load(Ordering::Relaxed) {
        let hdr = &mut headers[current_buf];

        // Wait until buffer is completed by the sound driver
        if (hdr.dw_flags & WHDR_DONE) == 0 {
            thread::sleep(Duration::from_millis(2));
            continue;
        }

        // Clear mixing buffers
        mix_left.fill(0.0);
        mix_right.fill(0.0);

        // Mix all active voices
        if let Ok(mut lock) = state.lock() {
            let listener = lock.listener;
            let master_vol = lock.volumes[SoundCategory::Master as usize];
            let cat_vols = lock.volumes;

            for voice in lock.voices.iter_mut() {
                if voice.finished {
                    continue;
                }

                let cat_vol = cat_vols[voice.category as usize];
                let combined_base = voice.volume * cat_vol * master_vol;
                if combined_base <= 0.0001 {
                    continue;
                }

                // Calculate 3D panning and distance attenuation
                let (gain_l, gain_r) = match voice.position {
                    None => (combined_base, combined_base),
                    Some(pos) => {
                        let delta = pos - listener.position;
                        let dist = delta.length();
                        if dist >= voice.max_distance {
                            (0.0, 0.0)
                        } else {
                            // Quadratic distance attenuation with smooth rolloff
                            let atten = (1.0 - (dist / voice.max_distance)).max(0.0).powi(2);
                            let dir = if dist > 0.001 { delta / dist } else { listener.forward };
                            let pan = dir.dot(listener.right).clamp(-1.0, 1.0);
                            let gl = ((1.0 - pan) * 0.5) * atten * combined_base;
                            let gr = ((1.0 + pan) * 0.5) * atten * combined_base;
                            (gl, gr)
                        }
                    }
                };

                let total_frames = voice.buffer.total_frames();
                if total_frames == 0 {
                    voice.finished = true;
                    continue;
                }

                let resample_ratio = (voice.buffer.sample_rate as f64 / MIX_SAMPLE_RATE as f64) * voice.pitch;

                for f in 0..BUFFER_SAMPLES {
                    // Apply fade multiplier (for ambient crossfades)
                    if voice.fade_delta != 0.0 {
                        voice.fade_multiplier = (voice.fade_multiplier + voice.fade_delta).clamp(0.0, 1.0);
                        if voice.fade_multiplier <= 0.0 && voice.fade_delta < 0.0 {
                            voice.finished = true;
                            break;
                        }
                    }

                    let frame_idx = voice.cursor as usize;
                    if frame_idx >= total_frames {
                        if voice.looping {
                            voice.cursor %= total_frames as f64;
                        } else {
                            voice.finished = true;
                            break;
                        }
                    }

                    let (sl, sr) = voice.buffer.sample_linear(voice.cursor);
                    let effective_l = gain_l * voice.fade_multiplier;
                    let effective_r = gain_r * voice.fade_multiplier;

                    mix_left[f] += sl * effective_l;
                    mix_right[f] += sr * effective_r;

                    voice.cursor += resample_ratio;
                }
            }

            // Remove finished voices
            lock.voices.retain(|v| !v.finished);
        }

        // Convert mixed f32 samples to 16-bit PCM and write into buffer
        let out_bytes = &mut raw_buffers[current_buf];
        for f in 0..BUFFER_SAMPLES {
            let left_i16 = (mix_left[f].clamp(-1.0, 1.0) * 32767.0) as i16;
            let right_i16 = (mix_right[f].clamp(-1.0, 1.0) * 32767.0) as i16;

            let byte_idx = f * 4;
            let [l0, l1] = left_i16.to_le_bytes();
            let [r0, r1] = right_i16.to_le_bytes();

            out_bytes[byte_idx] = l0;
            out_bytes[byte_idx + 1] = l1;
            out_bytes[byte_idx + 2] = r0;
            out_bytes[byte_idx + 3] = r1;
        }

        hdr.dw_flags &= !WHDR_DONE;
        unsafe {
            waveOutWrite(h_wave_out, hdr, std::mem::size_of::<WAVEHDR>() as u32);
        }

        current_buf = (current_buf + 1) % NUM_BUFFERS;
    }

    // Cleanup when engine stops
    unsafe {
        waveOutReset(h_wave_out);
        for hdr in &mut headers {
            if (hdr.dw_flags & WHDR_PREPARED) != 0 {
                waveOutUnprepareHeader(h_wave_out, hdr, std::mem::size_of::<WAVEHDR>() as u32);
            }
        }
        waveOutClose(h_wave_out);
        timeEndPeriod(1);
    }
}
