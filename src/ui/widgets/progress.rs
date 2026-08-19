use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::crawler::CrawlStatus;
use crate::runner::ProgressEvent;

/// One line of the rolling download log shown in the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadLogEntry {
    /// Chapter was written to disk.
    Ok(u32),
    /// Chapter file already existed and was skipped.
    Skip(u32),
    /// Chapter download failed. Carries the chapter number and the error
    /// message so the rolling log shows *why* it failed, not just that it did.
    Fail(u32, String),
    /// A run-scoped notice that belongs to no single chapter, e.g. the
    /// source's rate policy capping the worker count.
    Note(String),
}

/// Default number of recent log entries kept for display.
const DEFAULT_LOG_WINDOW: usize = 500;

/// How many recent chapter arrivals the remaining-time estimate is measured
/// over. Deliberately short: a whole-run average is poisoned for the rest of the
/// run by a burst of already-on-disk chapters, whereas a window that old
/// arrivals fall out of recovers on its own.
const MAX_RATE_SAMPLES: usize = 20;

/// Shortest sample span the estimate will be computed from. A sample count alone
/// does not protect against bursts: 900 skipped chapters, or a parallel run's
/// first wave of workers, can fill the whole window within milliseconds and
/// imply a rate of hundreds per second. Reporting nothing beats reporting a
/// confident `00:00:00` just as the real work begins.
const MIN_RATE_SPAN: Duration = Duration::from_secs(2);

/// Render `duration` as `HH:MM:SS`, matching the non-interactive progress bar's
/// `elapsed_precise`. The hour field is unconditional, since runs regularly
/// cross an hour and a shorter form would only need a fallback branch. Hours are
/// never wrapped, so a very long run keeps counting past 24.
pub fn format_hms(duration: Duration) -> String {
    let total = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

/// Mutable state backing the in-TUI download progress screen.
///
/// The progress callback installed on the runner pushes events into one of
/// these via `from_event`; the render loop reads it on every redraw tick.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Total chapters expected in this run (used for the gauge denominator).
    pub total: u32,
    /// Chapters written successfully so far.
    pub completed: u32,
    /// Chapters that produced an error so far.
    pub failed: u32,
    /// The chapter the most recent `Started` event referenced, if any.
    pub current_chapter: Option<u32>,
    /// Rolling window of the most recent log entries.
    pub log: Vec<DownloadLogEntry>,
    /// Maximum number of log entries kept in `log`.
    pub log_capacity: usize,
    /// Set when the runner has finished — flips the screen into "done" mode.
    pub done: bool,
    /// When the run began. Elapsed time is measured from here rather than from
    /// the first chapter event, so it is reported while the run is still
    /// starting up.
    pub started_at: Instant,
    /// When the run ended, once it has. Freezes elapsed time at that instant so
    /// the final screen reports the run's own duration rather than how long the
    /// user has been reading it.
    pub finished_at: Option<Instant>,
    /// Arrival instants of the most recent terminal chapter events, oldest
    /// first, capped at [`MAX_RATE_SAMPLES`]. Ordering holds because the
    /// progress callback stamps each arrival while holding the shared mutex, so
    /// parallel workers cannot interleave out of order.
    pub completions: VecDeque<Instant>,
}

impl DownloadProgress {
    /// Construct an empty progress state for a run of `total` chapters.
    pub fn new(total: u32) -> Self {
        Self::with_log_capacity(total, DEFAULT_LOG_WINDOW)
    }

    /// Same as [`new`] but with a custom log window size.
    pub fn with_log_capacity(total: u32, log_capacity: usize) -> Self {
        Self {
            total,
            completed: 0,
            failed: 0,
            current_chapter: None,
            log: Vec::with_capacity(log_capacity),
            log_capacity,
            done: false,
            started_at: Instant::now(),
            finished_at: None,
            completions: VecDeque::with_capacity(MAX_RATE_SAMPLES),
        }
    }

    /// Record that chapter `number` is about to start downloading.
    pub fn record_started(&mut self, number: u32) {
        self.current_chapter = Some(number);
    }

    /// Record a successful (or skipped) chapter completion.
    pub fn record_completed(&mut self, number: u32, status: CrawlStatus) {
        self.record_completed_at(number, status, Instant::now());
    }

    /// Record a successful (or skipped) chapter completion that arrived at
    /// `now`, contributing one sample to the throughput window.
    pub fn record_completed_at(&mut self, number: u32, status: CrawlStatus, now: Instant) {
        self.completed += 1;
        let entry = match status {
            CrawlStatus::Written => DownloadLogEntry::Ok(number),
            CrawlStatus::Skipped | CrawlStatus::SkipAll => DownloadLogEntry::Skip(number),
        };
        self.push_log(entry);
        self.push_sample(now);
    }

    /// Record a failed chapter download. `message` is the runner-supplied
    /// error text rendered alongside the chapter number in the TUI log.
    pub fn record_failed(&mut self, number: u32, message: String) {
        self.record_failed_at(number, message, Instant::now());
    }

    /// Record a failed chapter download that arrived at `now`. Failures advance
    /// the run just as completions do, so they contribute a sample too.
    pub fn record_failed_at(&mut self, number: u32, message: String, now: Instant) {
        self.failed += 1;
        self.push_log(DownloadLogEntry::Fail(number, message));
        self.push_sample(now);
    }

    /// Number of arrivals currently held in the throughput window.
    pub fn rate_samples(&self) -> usize {
        self.completions.len()
    }

    /// Record a run-scoped notice. Counts towards neither `completed` nor
    /// `failed`: nothing about the chapter tally has changed.
    pub fn record_note(&mut self, message: String) {
        self.push_log(DownloadLogEntry::Note(message));
    }

    /// Mark the run as done so the TUI flips into "press Enter to continue" mode.
    pub fn finish(&mut self) {
        self.finish_at(Instant::now());
    }

    /// Mark the run as done as of `now`, freezing elapsed time at that instant.
    pub fn finish_at(&mut self, now: Instant) {
        self.done = true;
        self.finished_at = Some(now);
    }

    /// Wall-clock time this run has taken as of `now`, or its final duration
    /// once the run has ended.
    pub fn elapsed(&self, now: Instant) -> Duration {
        self.finished_at
            .unwrap_or(now)
            .saturating_duration_since(self.started_at)
    }

    /// Estimated time remaining as of `now`, or `None` when the samples do not
    /// support an estimate.
    ///
    /// Throughput is measured over the arrival window rather than the whole run,
    /// and the span is stretched to `now` so a stalled run's estimate grows
    /// instead of reporting the figure that was current when the last chapter
    /// landed.
    pub fn eta(&self, now: Instant) -> Option<Duration> {
        if self.done || self.total == 0 {
            return None;
        }
        let remaining = self.total.saturating_sub(self.advanced());
        if remaining == 0 {
            return None;
        }
        let oldest = *self.completions.front()?;
        let newest = *self.completions.back()?;
        let intervals = self.completions.len().checked_sub(1)?;
        if intervals == 0 {
            return None;
        }
        let span = newest
            .saturating_duration_since(oldest)
            .max(now.saturating_duration_since(oldest));
        if span < MIN_RATE_SPAN {
            return None;
        }
        let seconds_per_chapter = span.as_secs_f64() / intervals as f64;
        Some(Duration::from_secs_f64(
            seconds_per_chapter * f64::from(remaining),
        ))
    }

    /// Total chapters with a terminal event observed (completed + failed).
    pub fn advanced(&self) -> u32 {
        self.completed + self.failed
    }

    /// Apply a runner [`ProgressEvent`] to this state.
    pub fn from_event(&mut self, event: ProgressEvent) {
        match event {
            ProgressEvent::Started { number, .. } => self.record_started(number),
            ProgressEvent::Completed { number, status } => {
                self.record_completed_at(number, status, Instant::now())
            }
            ProgressEvent::Failed { number, message } => {
                self.record_failed_at(number, message, Instant::now())
            }
            ProgressEvent::ConcurrencyClamped {
                requested,
                effective,
                source,
            } => self.record_note(format!(
                "{source} allows at most {effective} concurrent requests: using {effective} workers instead of {requested}"
            )),
        }
    }

    /// Percentage complete (0..=100), rounded.
    pub fn percent(&self) -> u16 {
        if self.total == 0 {
            return 100;
        }
        let ratio = self.advanced() as f64 / self.total as f64;
        (ratio.clamp(0.0, 1.0) * 100.0).round() as u16
    }

    /// Push an arrival instant while preserving the throughput window's cap.
    fn push_sample(&mut self, now: Instant) {
        self.completions.push_back(now);
        while self.completions.len() > MAX_RATE_SAMPLES {
            self.completions.pop_front();
        }
    }

    /// Push `entry` while preserving the rolling-window invariant.
    fn push_log(&mut self, entry: DownloadLogEntry) {
        self.log.push(entry);
        while self.log.len() > self.log_capacity {
            self.log.remove(0);
        }
    }
}

/// Compose the download gauge's label from `progress` as of `now`.
///
/// While the run is in flight the estimate segment is always present, showing a
/// placeholder when no estimate is available, so the label keeps a stable width
/// from one redraw to the next. Once the run has ended the segment is dropped
/// entirely, since nothing redraws after that.
pub fn gauge_label(progress: &DownloadProgress, now: Instant) -> String {
    let head = format!(
        "{} / {}  ({}%)  ⏱ {}",
        progress.advanced(),
        progress.total,
        progress.percent(),
        format_hms(progress.elapsed(now))
    );
    if progress.done {
        return head;
    }
    let estimate = match progress.eta(now) {
        Some(remaining) => format_hms(remaining),
        None => "—".to_string(),
    };
    format!("{head}  ETA {estimate}")
}

/// Build a runner progress callback that updates `state` from each event.
pub fn make_tui_progress_callback(
    state: Arc<std::sync::Mutex<DownloadProgress>>,
) -> crate::runner::ProgressCallback {
    Arc::new(move |event| {
        if let Ok(mut guard) = state.lock() {
            guard.from_event(event);
        }
    })
}
