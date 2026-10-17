//! Peak meters. Each meter is a low-rate `parec` capture of a source; a reader
//! thread folds samples into a peak that the UI takes once per frame.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Meter {
    peak: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    /// Smoothed level shown in the UI, 0..=1 linear amplitude.
    display: f32,
}

impl Meter {
    fn start(source: String) -> Self {
        let peak = Arc::new(AtomicU32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(None::<Child>));
        let (p, s, c) = (peak.clone(), stop.clone(), child.clone());
        std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                let spawned = Command::new("parec")
                    .args([
                        "-d", &source,
                        "--rate=8000", "--channels=1", "--format=float32le",
                        "--latency-msec=30", "--raw",
                        "--client-name=LinuxLink meter",
                        "--property=application.id=linuxlink",
                    ])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn();
                if let Ok(mut proc) = spawned {
                    let mut out = proc.stdout.take().unwrap();
                    *c.lock().unwrap() = Some(proc);
                    let mut buf = [0u8; 1024];
                    while let Ok(n @ 1..) = out.read(&mut buf) {
                        let max = buf[..n - n % 4]
                            .as_chunks::<4>().0.iter()
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs())
                            .fold(0.0f32, f32::max);
                        // Non-negative floats order the same as their bit patterns.
                        p.fetch_max(max.min(4.0).to_bits(), Ordering::Relaxed);
                    }
                    if let Some(mut proc) = c.lock().unwrap().take() {
                        let _ = proc.kill();
                        let _ = proc.wait();
                    }
                }
                // Source missing or parec died; retry shortly.
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        Self { peak, stop, child, display: 0.0 }
    }

    fn tick(&mut self, dt: f32) -> f32 {
        let peak = f32::from_bits(self.peak.swap(0, Ordering::Relaxed));
        // Instant attack, ~1.5 s fall from full scale.
        self.display = peak.max(self.display * (-dt * 3.0).exp());
        self.display
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut proc) = self.child.lock().unwrap().take() {
            let _ = proc.kill();
            let _ = proc.wait();
        }
    }
}

#[derive(Default)]
pub struct MeterBank {
    meters: HashMap<String, Meter>,
    levels: HashMap<String, f32>,
}

impl MeterBank {
    /// Starts/stops meters so exactly `sources` are measured, and advances them.
    pub fn update<'a>(&mut self, sources: impl IntoIterator<Item = &'a str>, dt: f32) {
        let wanted: Vec<&str> = sources.into_iter().filter(|s| !s.is_empty()).collect();
        self.meters.retain(|k, _| wanted.contains(&k.as_str()));
        for s in wanted {
            self.meters.entry(s.to_string()).or_insert_with(|| Meter::start(s.to_string()));
        }
        self.levels = self.meters.iter_mut().map(|(k, m)| (k.clone(), m.tick(dt))).collect();
    }

    pub fn level(&self, source: &str) -> f32 {
        self.levels.get(source).copied().unwrap_or(0.0)
    }
}
