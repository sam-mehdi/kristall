use once_cell::sync::OnceCell;
use samplerate::{convert, ConverterType};
use std::{env, error::Error, time::Instant};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

static GLOBAL_CTX: OnceCell<WhisperContext> = OnceCell::new();

// To initialize the speech-to-text model only once, use a global context
fn initialize_global_context() {
    let model_path = env::var("WHISPER_MODEL_PATH").expect("Failed to get WHISPER_MODEL_PATH env var");
    let ctx = WhisperContext::new_with_params(model_path.as_str(), WhisperContextParameters::new())
		.expect("failed to load model");
	GLOBAL_CTX.set(ctx).expect("failed to set models");
}

pub struct UserInputHandler<'a> {
    model: WhisperState<'a>,
}

impl<'a> UserInputHandler<'a> {
    /// Initialize the Whisper model
    pub fn new () -> Self {
        let start = Instant::now();

        if GLOBAL_CTX.get().is_none() {
            initialize_global_context();
        }
        
        let state = GLOBAL_CTX.get().unwrap().create_state().unwrap();
        println!("\n==== Initialized Whisper in {:?} ====\n", start.elapsed());

        UserInputHandler {
            model: state
        }
    }

    /// Collect audio from default microphone on device. Returns audio samples after user hits enter
    fn get_audio_samples() -> Result<Vec<i16>, Box<dyn Error>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;
        println!(
            "Using microphone: {}",
            device
                .name()
                .unwrap_or_else(|_| "Unknown device".to_string())
        );
    
        let config = device.default_input_config()?;
    
        let err_fn = |err| eprintln!("An error occurred on stream: {:?}", err);
    
        // Create a shared buffer to hold samples
        let samples: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
        let samples_clone = samples.clone();
    
        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let mut samples = samples_clone.lock().unwrap();
                samples.extend_from_slice(data);
            },
            err_fn,
            None,
        )?;
    
        stream.play()?;
        println!("Recording... Press Enter to stop");

        // Once "Enter" is received, process the input
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
    
        // Stop the stream and drop it
        drop(stream);
    
        let locked_samples = samples.lock().unwrap();
        let captured_samples = locked_samples.clone();
        Ok(captured_samples)
    }

    /// Sends collected audio samples to Whisper. Samples should be mono
    fn get_text_from_samples(self: &mut Self, float_audio: &Vec<f32>) -> Result<String, Box<dyn std::error::Error>> {
        // Create a params object for running the model
        // The number of past samples to consider defaults to 0
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 0 });
    
        // Prepare Whisper model and set the language
        params.set_n_threads(2);
        let stt_language = env::var("STT_LANGUAGE").unwrap_or(String::from("auto"));
        params.set_language(Some(stt_language.as_str()));

        // Disable anything that prints to stdout
        params.set_print_progress(false);
        params.set_print_timestamps(false);
    
        let audio_start = Instant::now();

        // Call the model with the collected audio
        self.model.full(params, &float_audio[..])?;
    
        // Iterate through the segments of the transcript
        let mut full_text: String = String::new();
        let num_segments = self.model
            .full_n_segments()
            .expect("failed to get number of segments");
        for i in 0..num_segments {
            // Get the transcribed text and timestamps for the current segment
            let segment = self.model
                .full_get_segment_text(i)
                .expect("failed to get segment");

            full_text.push_str(&segment);
        }
    
        let audio_duration = audio_start.elapsed(); // Check how much time has elapsed
        println!("Speech-to-text took: {:?}", audio_duration);
    
        Ok(full_text)
    }

    /// Resample the audio sample rate
    fn resample_audio(
        input_samples: &[f32],
        from_rate: u32,
        to_rate: u32,
    ) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let channels = 2; // Stereo input
    
        // Retrieve and write samples to WAV
        let resampled_samples = convert(
            from_rate,
            to_rate,
            channels,
            ConverterType::SincBestQuality,
            input_samples,
        )?;
    
        Ok(resampled_samples)
    }
    
    /// Collects user audio and returns recognized text
    pub fn get_chatgpt_input(self: &mut Self) -> Result<String, Box<dyn std::error::Error>> {
        let samples = Self::get_audio_samples().expect("Failed to get samples");
        let mut float_samples = vec![0.0f32; samples.len()];
        whisper_rs::convert_integer_to_float_audio(&samples, &mut float_samples)
            .expect("Failed to convert samples to float");
    
        // Resample to 16KHz, since Whisper is trained on that
        let resampled_float_samples = Self::resample_audio( &float_samples, 48000, 16000).unwrap();

        // Whisper performs MUCH better with mono audio
        let mono_audio = whisper_rs::convert_stereo_to_mono_audio(&resampled_float_samples).unwrap();
    
        self.get_text_from_samples(&mono_audio)
    }
}
