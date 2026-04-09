//! メインスレッドの応答性を監視するウォッチドッグ。
//!
//! メインスレッドが毎フレーム [`Heartbeat::tick`] を呼び、ウォッチドッグスレッドが
//! 定期的にハートビートを確認する。一定時間更新がなければ「応答なし」と判定しログに記録する。
//!
//! 最小化中など `update()` が呼ばれない状況では [`Heartbeat::pause`] で監視を一時停止し、
//! 誤検知を防ぐ。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// pause 状態を示す番兵値（エポックミリ秒としてはあり得ない値）
const PAUSED: u64 = u64::MAX;

/// メインスレッドのハートビート（エポックミリ秒を AtomicU64 で共有）
#[derive(Clone)]
pub struct Heartbeat(Arc<AtomicU64>);

impl Heartbeat {
    fn new() -> Self {
        Self(Arc::new(AtomicU64::new(epoch_millis())))
    }

    /// 毎フレーム呼び出して現在時刻を記録する（監視有効化）
    pub fn tick(&self) {
        self.0.store(epoch_millis(), Ordering::Relaxed);
    }

    /// 監視を一時停止する（最小化時など `update()` が保証されない場面で使用）
    pub fn pause(&self) {
        self.0.store(PAUSED, Ordering::Relaxed);
    }

    fn load(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// ウォッチドッグスレッドを起動し、[`Heartbeat`] を返す。
///
/// - `threshold`: この時間ハートビートが更新されなければ警告を出す
/// - `interval`: チェック間隔
pub fn start(threshold: Duration, interval: Duration) -> Heartbeat {
    let heartbeat = Heartbeat::new();
    let hb = heartbeat.clone();
    let threshold_ms = threshold.as_millis() as u64;

    std::thread::Builder::new()
        .name("watchdog".into())
        .spawn(move || {
            let mut was_unresponsive = false;
            let mut freeze_start: u64 = 0;

            loop {
                std::thread::sleep(interval);
                let last = hb.load();

                // pause 中は監視スキップ（最小化・非アクティブ等）
                if last == PAUSED {
                    if was_unresponsive {
                        log::info!("[watchdog] Monitoring paused (window minimized/inactive)");
                        was_unresponsive = false;
                    }
                    continue;
                }

                let now = epoch_millis();
                let elapsed = now.saturating_sub(last);

                if elapsed > threshold_ms {
                    if !was_unresponsive {
                        freeze_start = last;
                        log::warn!(
                            "[watchdog] Main thread unresponsive (no heartbeat for {elapsed}ms)"
                        );
                        was_unresponsive = true;
                    } else {
                        let total = now.saturating_sub(freeze_start);
                        log::warn!("[watchdog] Main thread still unresponsive (total {total}ms)");
                    }
                } else if was_unresponsive {
                    let total = now.saturating_sub(freeze_start);
                    log::info!("[watchdog] Main thread recovered after {total}ms freeze");
                    was_unresponsive = false;
                }
            }
        })
        .expect("watchdog thread spawn failed");

    heartbeat
}
