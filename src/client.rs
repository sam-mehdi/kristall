use base64::decode;
use futures::{stream::SplitStream, SinkExt};
use rodio::{Decoder, OutputStream, Sink};
use serde::{Deserialize, Serialize};
use chatgpt::{client::ChatGPT, types::ResponseChunk};
use serde_json::{json, Value};
use std::{env, io::Cursor, sync::Arc};
use futures::stream::StreamExt;
use futures_util::stream::SplitSink;
use tokio::{sync::Mutex, task::JoinError};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, WebSocketStream, MaybeTlsStream};
use tokio::net::TcpStream;

// Define WebSocket endpoint and request structure - https://elevenlabs.io/docs/api-reference/websockets
const ELEVENLABS_WSS_ENDPOINT_BASE: &str = "wss://api.elevenlabs.io/v1/text-to-speech/{voice_id}/stream-input?model_id={model}&optimize_streaming_latency=1";

#[derive(Serialize, Deserialize)]
struct ElevenLabsInitializationData {
    text: String,
    voice_settings: ElevenLabsVoiceSettings,
    xi_api_key: String,
}
#[derive(Serialize, Deserialize)]
struct ElevenLabsVoiceSettings {
    stability: f64,
    similarity_boost: f64,
}

pub struct ChatGptElevenLabsClient {
    chatgpt_client: ChatGPT,
    elevenlabs_websocket_writer: Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
}

pub struct ElevenLabsListenerStreamer {
    pub sink: Arc<Sink>,
    #[allow(dead_code)]
    stream: OutputStream, // Keep stream alive
    websocket_reader: Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
}


impl ChatGptElevenLabsClient {
    pub async fn new() -> (Self, Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>) {
        // Create ChatGPT client
        let open_ai_key = env::var("OPENAI_API_KEY").expect("Failed to get OPENAI_API_KEY env var");
        let chatgpt_client = ChatGPT::new(open_ai_key ).expect("Failed to create ChatGPT client");
        
        // Create websocket to ElevenLabs
        let xi_voice_id = env::var("ELEVENLABS_VOICE_ID").expect("Failed to get ELEVENLABS_VOICE_ID env var");
        let xi_model_id = env::var("ELEVENLABS_MODEL_ID").expect("Failed to get ELEVENLABS_MODEL_ID env var");
        let xi_complete_wss_endpoint = ELEVENLABS_WSS_ENDPOINT_BASE.replace("{voice_id}", &xi_voice_id[..]).replace("{model}", &xi_model_id[..]);

        let (elevenlabs_ws_stream, _) = connect_async(xi_complete_wss_endpoint).await.expect("Failed to connect");
        let (elevenlabs_ws_writer, elevenlabs_ws_reader) = elevenlabs_ws_stream.split();
        let mutexed_elevenlabs_ws_writer = Mutex :: new(elevenlabs_ws_writer);

        // Send data to ElevenLabs to authenticate/initialize connection
        let xi_api_key = env::var("ELEVENLABS_API_KEY").expect("Failed to get ELEVENLABS_API_KEY env var");
        let data = ElevenLabsInitializationData {
            text: " ".to_string(),
            xi_api_key,
            voice_settings: ElevenLabsVoiceSettings {
                similarity_boost: 0.8,
                stability: 0.9
            }
        };
        let msg = serde_json::to_string(&data).unwrap();

        // Send the message as text
        let mut lock = mutexed_elevenlabs_ws_writer.lock().await;
        lock.send(Message::Text(msg)).await.expect("Failed to send initialization data to ElevenLabs");
        drop(lock);
        
        (ChatGptElevenLabsClient {
            chatgpt_client,
            elevenlabs_websocket_writer: mutexed_elevenlabs_ws_writer,
        }, Mutex::new(elevenlabs_ws_reader))
    }

    /// Sends `message` to ChatGPT. Streams ChatGPT response to ElevenLabs for audio
    pub async fn send_message_and_stream_audio(&self, message: String) {
        let chatgpt_stream = self.chatgpt_client.send_message_streaming(message).await.unwrap();
        let accumulated_text = Arc::new(Mutex::new(String::new()));
    
        // Iterating over stream contents
        chatgpt_stream
            .for_each({
                let accumulated_text = accumulated_text.clone();
                move |each| {
                    let accumulated_text = accumulated_text.clone();
                    async move {
                        match each {
                            ResponseChunk::Content { delta, response_index: _ } => {
                                let mut text = accumulated_text.lock().await;
                                text.push_str(&delta);

                                // Check if there are 30 spaces in the accumulated text. This should be a good chunk of text to give
                                // ElevenLabs enough context for the voice intonation
                                let space_count = text.chars().filter(|&c| c == ' ').count();
                                if space_count >= 30 {
                                    // Send the accumulated text and then clear it
                                    let data = json!({
                                        "text": &*text,
                                    });
                                    let msg = serde_json::to_string(&data).expect("Failed to serialize message");

                                    println!("Sending to elevenlabs: {}", text);
                                    let mut lock = self.elevenlabs_websocket_writer.lock().await;
                                    lock.send(Message::Text(msg)).await.expect("Failed to send message to ElevenLabs");

                                    text.clear(); // Clear the text directly
                                }
                            }
                            _ => {}
                        }
                    }
                }
            })
            .await;

        // Send any remaining text if it's not empty
        let remaining_text = accumulated_text.lock().await;
        if !remaining_text.is_empty() {
            let data = json!({ "text": &*remaining_text });
            let msg = serde_json::to_string(&data).expect("Failed to serialize message");

            println!("Sending to elevenlabs: {}", remaining_text);
            let mut lock = self.elevenlabs_websocket_writer.lock().await;
            lock.send(Message::Text(msg)).await.expect("Failed to send remaining message to ElevenLabs");
        }

        // Send the "close stream" message
        let data = json!({ "text": "" });
        let msg = serde_json::to_string(&data).expect("Failed to serialize message");
        let mut lock = self.elevenlabs_websocket_writer.lock().await;
        lock.send(Message::Text(msg)).await.expect("Failed to send closing message to ElevenLabs");
    }
}

impl ElevenLabsListenerStreamer {
    pub async fn new(reader: Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>) -> Result<Self, Box<dyn std::error::Error>> {
        let (stream, handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&handle)?;

        Ok(ElevenLabsListenerStreamer {
            sink: Arc::new(sink),
            stream,
            websocket_reader: reader,
        })
    }

    /// Listens for incoming audio from ElevenLabs and plays it to the user
    pub async fn listen_and_play(&self) -> Result<(), JoinError> {
        let reader_clone = self.websocket_reader.clone();
        let sink = self.sink.clone();
        
        let task = tokio::spawn(async move {
            let mut reader = reader_clone.lock().await;

            while let Some(message) = reader.next().await {
                match message.unwrap() {
                    Message::Text(text) => {
                        let response: Value = serde_json::from_str(&text).unwrap();
                        if let Some(audio_base64) = response["audio"].as_str() {
                            let decoded_audio = decode(audio_base64).unwrap();
                            let cursor = Cursor::new(decoded_audio);
                            let source = Decoder::new(cursor).unwrap();

                            sink.append(source);
                        } else if let Some(error_message) = response["message"].as_str() {
                            panic!("ERROR OCCURRED IN ELEVENLABS: {}", error_message);
                        }
                    },
                    _ => {}
                }
            }

            sink.sleep_until_end();
        });

        task.await
    }
}
