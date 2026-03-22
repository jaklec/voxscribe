use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;

use crate::recorder::Recorder;

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub fn run_interactive(recorder: Recorder) -> Result<PathBuf> {
    let handle = crate::recorder::start_recording(recorder)?;

    let raw_guard = RawModeGuard::enable()?;
    let result = interactive_loop(&handle);
    drop(raw_guard);

    eprint!("\r\x1b[K");

    match result {
        Ok(()) => {
            eprintln!("Transcribing...");
            let wav_path = handle.stop()?;
            Ok(wav_path)
        }
        Err(e) => {
            let _ = handle.stop();
            Err(e)
        }
    }
}

fn interactive_loop(handle: &crate::recorder::RecordingHandle) -> Result<()> {
    let mut timer = RecordingTimer::new();

    loop {
        display_status(handle.is_paused(), timer.elapsed());

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key {
                    KeyEvent {
                        code: KeyCode::Char(' '),
                        ..
                    } => {
                        if handle.is_paused() {
                            handle.resume();
                            timer.resume();
                        } else {
                            handle.pause();
                            timer.pause();
                        }
                    }
                    KeyEvent {
                        code: KeyCode::Char('q'),
                        ..
                    }
                    | KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }
}

fn display_status(paused: bool, elapsed: Duration) {
    let secs = elapsed.as_secs();
    let mins = secs / 60;
    let secs = secs % 60;

    let status = if paused { "Paused" } else { "Recording..." };
    eprint!("\r\x1b[K{status}  {mins:02}:{secs:02}  [space] pause/resume  [q] stop");
    let _ = std::io::stderr().flush();
}

pub struct RecordingTimer {
    active_duration: Duration,
    segment_start: Option<Instant>,
}

impl RecordingTimer {
    pub fn new() -> Self {
        Self {
            active_duration: Duration::ZERO,
            segment_start: Some(Instant::now()),
        }
    }

    pub fn pause(&mut self) {
        if let Some(start) = self.segment_start.take() {
            self.active_duration += start.elapsed();
        }
    }

    pub fn resume(&mut self) {
        if self.segment_start.is_none() {
            self.segment_start = Some(Instant::now());
        }
    }

    pub fn elapsed(&self) -> Duration {
        let current = self
            .segment_start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO);
        self.active_duration + current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_starts_running() {
        let timer = RecordingTimer::new();
        std::thread::sleep(Duration::from_millis(50));
        let elapsed = timer.elapsed();
        assert!(elapsed >= Duration::from_millis(40));
    }

    #[test]
    fn timer_pause_stops_counting() {
        let mut timer = RecordingTimer::new();
        std::thread::sleep(Duration::from_millis(50));
        timer.pause();
        let after_pause = timer.elapsed();
        std::thread::sleep(Duration::from_millis(50));
        let later = timer.elapsed();
        // Should be approximately the same since timer is paused
        let diff = later.as_millis() as i64 - after_pause.as_millis() as i64;
        assert!(diff.abs() < 5);
    }

    #[test]
    fn timer_resume_continues_counting() {
        let mut timer = RecordingTimer::new();
        std::thread::sleep(Duration::from_millis(50));
        timer.pause();
        let paused_elapsed = timer.elapsed();
        std::thread::sleep(Duration::from_millis(50));
        timer.resume();
        std::thread::sleep(Duration::from_millis(50));
        let final_elapsed = timer.elapsed();
        // Should have added ~50ms more after resume (not the 50ms during pause)
        assert!(final_elapsed > paused_elapsed);
        assert!(final_elapsed >= Duration::from_millis(90));
        assert!(final_elapsed < Duration::from_millis(200));
    }

    #[test]
    fn timer_multiple_pause_resume_cycles() {
        let mut timer = RecordingTimer::new();
        // Record 50ms
        std::thread::sleep(Duration::from_millis(50));
        timer.pause();
        // Pause 50ms
        std::thread::sleep(Duration::from_millis(50));
        timer.resume();
        // Record 50ms
        std::thread::sleep(Duration::from_millis(50));
        timer.pause();

        let elapsed = timer.elapsed();
        // Should be ~100ms (two 50ms segments), not 150ms
        assert!(elapsed >= Duration::from_millis(80));
        assert!(elapsed < Duration::from_millis(160));
    }

    #[test]
    fn timer_double_pause_is_idempotent() {
        let mut timer = RecordingTimer::new();
        std::thread::sleep(Duration::from_millis(50));
        timer.pause();
        let first = timer.elapsed();
        timer.pause(); // second pause should be no-op
        let second = timer.elapsed();
        let diff = second.as_millis() as i64 - first.as_millis() as i64;
        assert!(diff.abs() < 5);
    }

    #[test]
    fn timer_double_resume_is_idempotent() {
        let mut timer = RecordingTimer::new();
        timer.resume(); // already running, should be no-op
        std::thread::sleep(Duration::from_millis(50));
        let elapsed = timer.elapsed();
        assert!(elapsed >= Duration::from_millis(40));
        assert!(elapsed < Duration::from_millis(150));
    }
}
