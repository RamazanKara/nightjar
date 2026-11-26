//! Polling helper for Microphone-only mode: watches whether the PC's session is already
//! playing another device's microphone so the UI can surface and clear busy state.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::Waker;

/// How often Microphone only asks the PC whether another device's microphone plays.
pub(super) const MICROPHONE_BUSY_EVERY: Duration = Duration::from_secs(3);

/// A thread that asks the PC whether this device's microphone waits for another's, until
/// dropped with the stream.
pub(super) struct MicrophoneBusyWatch {
    busy: Arc<std::sync::atomic::AtomicBool>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl MicrophoneBusyWatch {
    /// `connect` makes the question; `wake` runs when the answer changes.
    pub(super) fn start<Q, F>(connect: F, wake: Waker, every: Duration) -> Option<Self>
    where
        F: FnOnce() -> Result<Q, String> + Send + 'static,
        Q: Fn() -> Result<bool, String>,
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        let busy = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, stopped) = (busy.clone(), stop.clone());
        let spawned = std::thread::Builder::new().name("microphone-busy".into()).spawn(move || {
            let ask = match connect() {
                Ok(ask) => ask,
                Err(error) => {
                    eprintln!("nightjar: microphone only: the PC's session cannot be read: {error}");
                    return;
                }
            };
            let mut last_error = None;
            while !stopped.load(Ordering::Acquire) {
                match ask() {
                    Ok(now) => {
                        last_error = None;
                        if seen.swap(now, Ordering::AcqRel) != now {
                            eprintln!("nightjar: microphone only: {}", if now { "another device's microphone plays on the PC" } else { "the PC plays this microphone again" });
                            wake();
                        }
                    }
                    // Said once per kind of failure; the next answer counts again.
                    Err(error) => {
                        if seen.swap(false, Ordering::AcqRel) {
                            wake();
                        }
                        if last_error.as_ref() != Some(&error) {
                            eprintln!("nightjar: microphone only: the PC's session could not be read: {error}");
                            last_error = Some(error);
                        }
                    }
                }
                let until = Instant::now() + every;
                while !stopped.load(Ordering::Acquire) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(100).min(every));
                }
            }
        });
        match spawned {
            Ok(_) => Some(Self { busy, stop }),
            Err(error) => {
                eprintln!("nightjar: microphone only: no thread to ask the PC about the microphone: {error}");
                None
            }
        }
    }

    pub(super) fn busy(&self) -> bool {
        self.busy.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl Drop for MicrophoneBusyWatch {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[test]
    fn the_watch_follows_the_pcs_answer_and_wakes_on_each_change() {
        let answers = Arc::new(Mutex::new(vec![Ok(true), Err("offline".to_owned()), Ok(true), Ok(false)]));
        let wakes = Arc::new(AtomicUsize::new(0));
        let woke = wakes.clone();
        let queue = answers.clone();
        let watch = MicrophoneBusyWatch::start(
            move || Ok(move || queue.lock().unwrap().pop().unwrap_or(Ok(false))),
            Arc::new(move || { woke.fetch_add(1, Ordering::SeqCst); }),
            Duration::from_millis(5),
        )
        .expect("a watch");
        // Answers pop from the end: false, true, an error, true; then false for good.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !answers.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(50));
        assert!(!watch.busy());
        // false (initial), true (wake 1), error clears busy (wake 2), true (wake 3), false (wake 4).
        assert_eq!(wakes.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn a_pc_that_cannot_be_reached_leaves_it_not_busy() {
        let watch = MicrophoneBusyWatch::start(
            || Err::<fn() -> Result<bool, String>, _>("no address".to_owned()),
            Arc::new(|| {}),
            Duration::from_millis(5),
        )
        .expect("a watch");
        std::thread::sleep(Duration::from_millis(30));
        assert!(!watch.busy());
    }
}
