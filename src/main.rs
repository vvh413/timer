use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

const TIMER_STEP_MS: u64 = 200;
const TOGGLE_CHAR: char = '\n';

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
  line_start: String,
  line_end: String,
}

impl Timer {
  fn new(hours: u64, minutes: u64, seconds: u64, line_mode: bool) -> Self {
    Self {
      duration: Arc::new(Mutex::new(Duration::from_secs(seconds + minutes * 60 + hours * 3600))),
      pause: Arc::new(AtomicBool::new(false)),
      line_start: (if line_mode { "\r" } else { "" }).to_string(),
      line_end: (if line_mode { "" } else { "\n" }).to_string(),
    }
  }

  fn start(&self) -> JoinHandle<Result<()>> {
    let timer_step = Duration::from_millis(TIMER_STEP_MS);
    let mut interval = tokio::time::interval(timer_step);
    let duration = self.duration.clone();
    let pause = self.pause.clone();
    let line_start = self.line_start.clone();
    let line_end = self.line_end.clone();
    tokio::spawn(async move {
      let mut stdout = tokio::io::stdout();
      loop {
        let mut locked_duration = duration.lock().await;
        if locked_duration.is_zero() {
          break;
        }
        if !pause.load(Ordering::Relaxed) {
          if locked_duration.subsec_millis() == 0 {
            stdout
              .write_all(format!("{line_start}{}{line_end:>3}", humantime::format_duration(*locked_duration)).as_bytes())
              .await?;
            stdout.flush().await?;
          }

          *locked_duration = locked_duration.saturating_sub(timer_step);
        }
        interval.tick().await;
      }
      println!("{line_start}done");
      Ok(())
    })
  }

  fn toggle(&self) {
    self.pause.store(!self.pause.load(Ordering::Relaxed), Ordering::Relaxed);
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

  let mut buf = String::new();
  let mut stdin = BufReader::new(tokio::io::stdin());
  while !timer.done().await {
    match tokio::time::timeout(Duration::from_millis(TIMER_STEP_MS), stdin.read_line(&mut buf)).await {
      Ok(result) => result?,
      Err(_) => continue,
    };
    if buf == TOGGLE_CHAR.to_string() {
      buf = String::new();
      timer.toggle();
    }
  }

  timer_future.await??;
  exit(0)
}
