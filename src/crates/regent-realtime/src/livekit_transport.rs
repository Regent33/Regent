//! LiveKit/WebRTC transport for the realtime engine (ADR-021 R2). Feature-gated
//! (`livekit`) because it pulls native libwebrtc. The engine ([`crate::run_call`])
//! is transport-agnostic; this adapter joins a LiveKit room as the agent
//! participant, streams the caller's audio track into the engine, and publishes
//! the engine's audio back out — both as 24 kHz mono PCM (the provider's rate;
//! libwebrtc resamples to/from the room's 48 kHz at the edge).

use std::borrow::Cow;

use futures_util::StreamExt;
use livekit::options::TrackPublishOptions;
use livekit::track::{LocalAudioTrack, LocalTrack, RemoteTrack, TrackSource};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::AudioFrame as LkAudioFrame;
use livekit::{Room, RoomEvent, RoomOptions};
use tokio::sync::mpsc;

use crate::{AudioFrame, RealtimeError, TransportEnds};

/// How to reach the LiveKit room. `url` is the ws(s) signaling URL; `token` is a
/// join JWT (minted by the same `livekit-server-sdk` the web app's token route uses).
pub struct LiveKitConfig {
    pub url: String,
    pub token: String,
    /// Provider/engine PCM rate (OpenAI Realtime = 24 kHz).
    pub sample_rate: u32,
}

/// Join the room and return the engine's [`TransportEnds`] plus the live [`Room`]
/// (keep it alive for the call's duration — dropping it disconnects). Spawns the
/// I/O tasks that pump audio both ways.
pub async fn connect(cfg: LiveKitConfig) -> Result<(TransportEnds, Room), RealtimeError> {
    let (caller_tx, caller_rx) = mpsc::channel::<AudioFrame>(64); // caller → engine
    let (out_tx, mut out_rx) = mpsc::channel::<AudioFrame>(64); // engine → caller

    let (room, mut events) = Room::connect(&cfg.url, &cfg.token, RoomOptions::default())
        .await
        .map_err(|e| RealtimeError::Transport(e.to_string()))?;

    // Outbound: publish a mono track the engine writes into.
    let source = NativeAudioSource::new(
        AudioSourceOptions::default(),
        cfg.sample_rate,
        1,    // mono
        1000, // queue size (ms)
    );
    let track = LocalAudioTrack::create_audio_track("regent", RtcAudioSource::Native(source.clone()));
    room.local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
        )
        .await
        .map_err(|e| RealtimeError::Transport(e.to_string()))?;

    tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            let samples = frame.pcm.len() as u32;
            let lk = LkAudioFrame {
                data: Cow::Owned(frame.pcm),
                sample_rate: frame.sample_rate,
                num_channels: 1,
                samples_per_channel: samples,
            };
            if source.capture_frame(&lk).await.is_err() {
                break; // source gone
            }
        }
    });

    // Inbound: on each subscribed audio track, stream resampled frames to the engine.
    let sr = cfg.sample_rate;
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                RoomEvent::TrackSubscribed { track: RemoteTrack::Audio(audio), .. } => {
                    let tx = caller_tx.clone();
                    let mut stream = NativeAudioStream::new(audio.rtc_track(), sr as i32, 1);
                    tokio::spawn(async move {
                        while let Some(frame) = stream.next().await {
                            let af = AudioFrame {
                                pcm: frame.data.to_vec(),
                                sample_rate: frame.sample_rate,
                            };
                            if tx.send(af).await.is_err() {
                                break; // engine gone
                            }
                        }
                    });
                }
                RoomEvent::Disconnected { .. } => break,
                _ => {}
            }
        }
        // caller_tx drops here → engine's audio_in closes → call ends cleanly
    });

    Ok((TransportEnds { audio_in: caller_rx, audio_out: out_tx }, room))
}
