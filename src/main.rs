use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use eyre::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

const TIMER_STEP: Duration = Duration::from_millis(500);
const TOGGLE_CHAR: u8 = b'\n';

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Hours
    #[arg(short = 'H', long, default_value_t = 0)]
    hours: u64,

    /// Minutes
    #[arg(short, long, default_value_t = 0)]
    minutes: u64,

    /// Seconds
    #[arg(short, long, default_value_t = 0)]
    seconds: u64,

    /// Print elapsed time on single line
    #[arg(short, long)]
    line_mode: bool,
}

#[derive(Debug)]
struct Timer {
    duration: Arc<Mutex<Duration>>,
    pause: Arc<AtomicBool>,
    line_start: &'static str,
    line_end: &'static str,
}

impl Timer {
    fn new(hours: u64, minutes: u64, seconds: u64, line_mode: bool) -> Self {
        Self {
            duration: Arc::new(Mutex::new(Duration::from_secs(
                seconds + minutes * 60 + hours * 3600,
            ))),
            pause: Arc::new(AtomicBool::new(false)),
            line_start: if line_mode { "\r" } else { "" },
            line_end: if line_mode { "" } else { "\n" },
        }
    }

    fn start(&self) -> JoinHandle<Result<()>> {
        let mut interval = tokio::time::interval(TIMER_STEP);
        let duration = self.duration.clone();
        let pause = self.pause.clone();
        let line_start = self.line_start;
        let line_end = self.line_end;
        tokio::spawn(async move {
            let mut stdout = tokio::io::stdout();
            loop {
                interval.tick().await;
                let mut locked_duration = duration.lock().await;
                if locked_duration.is_zero() {
                    break;
                }
                if !pause.load(Ordering::Relaxed) {
                    if locked_duration.subsec_millis() == 0 {
                        let time = humantime::format_duration(*locked_duration);
                        stdout
                            .write_all(format!("{line_start}{time}{line_end:>3}").as_bytes())
                            .await?;
                        stdout.flush().await?;
                    }

                    *locked_duration = locked_duration.saturating_sub(TIMER_STEP);
                }
            }
            println!("{line_start}done");
            Ok(())
        })
    }

    fn toggle(&self) {
        self.pause
            .store(!self.pause.load(Ordering::Acquire), Ordering::Relaxed);
    }

    async fn done(&self) -> bool {
        self.duration.lock().await.is_zero()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args {
        hours,
        minutes,
        seconds,
        line_mode,
    } = Args::parse();
    let timer = Timer::new(hours, minutes, seconds, line_mode);
    let timer_future = timer.start();

    let mut stdin = tokio::io::stdin();
    while !timer.done().await {
        match tokio::time::timeout(TIMER_STEP, stdin.read_u8()).await {
            Ok(Ok(TOGGLE_CHAR)) => {
                timer.toggle();
            }
            _ => continue,
        };
    }

    timer_future.await??;
    std::process::exit(0)
}
