use eyre::Result;
use futures::StreamExt as _;
use std::fmt::Debug;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::time::DelayQueue;

#[derive(Debug)]
pub enum TimerCommand<T> {
    // todo should probably specifiy an instant instead or time or whatever
    AddTimer { data: T, when: Instant },
    Shutdown,
}

pub async fn timer_queue_worker<T, F>(
    mut rx: mpsc::Receiver<TimerCommand<T>>,
    mut handler: impl FnMut(T) -> F,
) where
    T: Debug + Send + 'static,
    F: Future<Output = Result<()>>,
{
    let mut delay_queue = DelayQueue::new();
    loop {
        tokio::select! {
            // handle new timers
            Some(cmd) = rx.recv() => match cmd {
                TimerCommand::AddTimer { data, when } => {
                    tracing::debug!("Adding timer {data:?}");
                    delay_queue.insert_at(data, when);
                }
                TimerCommand::Shutdown => {
                    tracing::debug!("Shutting down timer queue worker");
                    break;
                }
            },

            // handle expired timers
            Some(expired) = delay_queue.next() => {
                let item = expired.into_inner();
                tracing::debug!("⏰ Timer fired: {item:?}");
                if let Err(error) = handler(item).await{
                    tracing::error!("Error in timer queue worker: {error:?}");
                }
            },

            else => break,
        }
    }
}

pub fn spawn_timer_queue<T, F>(
    handler: impl FnMut(T) -> F + Send + 'static,
) -> mpsc::Sender<TimerCommand<T>>
where
    T: Debug + Send + 'static,
    F: Future<Output = Result<()>> + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<TimerCommand<T>>(100);
    tokio::spawn(timer_queue_worker(rx, handler));
    tx
}
