# kristall

A conversation partner for language learning. Sends ChatGPT-generated text to ElevenLabs for audio.

https://github.com/sam-mehdi/kristall/assets/69536392/ef2f2c58-3556-46b1-a1e0-d17e739e338e

Hit "Enter" to indicate that you're done talking.

Mostly made obsolete by ChatGPT's [new Voice Mode](https://help.openai.com/en/articles/8400625-voice-chat-faq).

## Setup

This section explains how to set up the `.env` file and the local speech-to-text model.

To start, rename `incomplete.env` to `.env`.

### Local speech-to-text model
OpenAI's [Whisper](https://github.com/openai/whisper) model is used to recognize the user's speech.

For the program, set up [whisper-rs](https://github.com/tazz4843/whisper-rs). Especially see [BUILDING.md](https://github.com/tazz4843/whisper-rs/blob/master/BUILDING.md). In `Cargo.toml`, make sure to import `whisper-rs` from the correct directory once you have it set up.

Example directory: `"C:\\Users\\...\\whisper-rs\\sys\\whisper.cpp\\models\\ggml-large-v3.bin"` (make sure to escape backslashes)

FYI, I got very good results using the large model.

### LLM
ChatGPT is used. Go [here](https://platform.openai.com/apps) and click "API". Log in or sign up. Then, in the left menu, click "API Keys". Create a new secret key and paste it in `OPENAI_API_KEY`.

### ElevenLabs voice
The program sends the LLM's output to ElevenLabs for audio.

Sign up for an ElevenLabs account [here](https://elevenlabs.io/). After you sign in, find your API key by hovering over your profile picture at the bottom left and clicking "Profile + API Key". Set `ELEVENLABS_API_KEY` with that API key.

Look through ElevenLabs' voice library [here](https://elevenlabs.io/app/voice-library). Once you've found the right voice, add it to your Voice Library. Click on the "Voices" tab again to go to VoiceLab. Click "ID" to copy the ID, and paste it in the `ELEVENLABS_VOICE_ID` variable.

Finally, you can change the model that Kristall uses. By default, `eleven_multilingual_v2` is used, but you can choose your own. Make a `Get Models` request in ElevenLabs [here](https://elevenlabs.io/docs/api-reference/get-models) using your API key, and paste the ID of the model you want (`model_id`) in `ELEVENLABS_MODEL_ID`.

## Running the app
`cargo run --release`

## Troubleshooting

### Whisper isn't understanding me!
Make sure to set your language (`STT_LANGUAGE` in `.env`). Also make sure your default system mic is correct.

### ElevenLabs' playback is choppy
`rodio`, the audio playback library in use, seems to have this issue when you're doing just `cargo run`. Adding the `release` option makes it much better, but not perfect. This is a significant limitation in the program.

## FAQ

### Kristall?
"Crystal" in German. Imagine all the facets of a crystal as different languages that the program can handle.

### What is this code?? And why did you choose Rust?
Look, I just wanted to try Rust, I'm sorry!
