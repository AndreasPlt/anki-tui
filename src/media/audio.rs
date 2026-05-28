use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Holds the audio output stream and sink for non-blocking playback.
/// Both must be kept alive for the duration of playback.
pub struct AudioPlayer {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    sink: Sink,
}

impl AudioPlayer {
    pub fn new() -> Option<Self> {
        let (stream, handle) = OutputStream::try_default().ok()?;
        let sink = Sink::try_new(&handle).ok()?;
        Some(Self {
            _stream: stream,
            _handle: handle,
            sink,
        })
    }

    /// Play audio files sequentially (non-blocking — playback runs in background).
    /// Stops any currently playing audio first.
    pub fn play(&self, paths: &[impl AsRef<Path>]) {
        self.sink.stop();
        for path in paths {
            let path = path.as_ref();
            if !path.exists() {
                continue;
            }
            if let Ok(file) = File::open(path)
                && let Ok(decoder) = Decoder::new(BufReader::new(file))
            {
                self.sink.append(decoder);
            }
        }
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        self.sink.stop();
    }
}
