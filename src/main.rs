use std::sync::Arc;

use dotenv;

mod client;
use client::{ChatGptElevenLabsClient, ElevenLabsListenerStreamer};
mod speech_to_text;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenv::dotenv().ok();

    // Initialize speech-to-text model once
    let mut user_input_handler = speech_to_text::UserInputHandler::new();
    
    loop {
        // Get the text-to-speech client
        let (client, wss_reader) = ChatGptElevenLabsClient::new().await;
        
        // Wrap in Arc so we can send it to another thread
        let client = Arc::new(client);
        let wss_reader = Arc::new(wss_reader);

        let chatgpt_input = user_input_handler.get_chatgpt_input().unwrap();
        println!("Sending text to ChatGPT: {}", chatgpt_input);

        let client_clone = client.clone();
        let wss_reader_clone = wss_reader.clone();

        // Spawn the text sender task
        tokio::spawn(async move {
            client_clone.send_message_and_stream_audio(chatgpt_input).await;
        });

        // Since wss_reader cannot be split or cloned, handle it in a way that does not require cloning the inner Mutex
        let listener_and_streamer = ElevenLabsListenerStreamer::new(wss_reader_clone).await.unwrap();
        let listener_task = async {
            listener_and_streamer.listen_and_play().await.expect("Failed to listen/stream");
        };

        /// Keep the program alive
        async fn periodic_log() {
            let mut interval = interval(Duration::from_secs(10000));
            loop {
                interval.tick().await;
            }
        }
        tokio::spawn(periodic_log());

        listener_task.await
    }
}
