//! 后台任务抽象：封装 thread::spawn + mpsc::channel 的通用模式
//!
//! @author sky

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};

/// 非阻塞轮询结果
pub enum Poll<T> {
    /// 结果就绪
    Ready(T),
    /// 任务仍在执行
    Pending,
    /// 后台线程已断开（通常是 panic）
    Lost,
}

/// 非阻塞轮询能力
pub trait Pollable<T> {
    /// 获取内部 channel 接收端
    fn rx(&self) -> &mpsc::Receiver<T>;

    /// 非阻塞轮询结果
    fn poll(&self) -> Poll<T> {
        match self.rx().try_recv() {
            Ok(v) => Poll::Ready(v),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => Poll::Lost,
        }
    }
}

/// 后台一次性任务
///
/// 封装 `thread::spawn` + `mpsc::channel` 的一次性通信模式，
/// 通过 `poll()` 非阻塞获取结果。
pub struct Task<T> {
    /// 结果接收端
    rx: mpsc::Receiver<T>,
}

impl<T: Send + 'static> Task<T> {
    /// 在后台线程执行闭包，返回任务句柄
    pub fn spawn(f: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        Self { rx }
    }
}

impl<T> Pollable<T> for Task<T> {
    fn rx(&self) -> &mpsc::Receiver<T> {
        &self.rx
    }
}

#[allow(dead_code)]
/// 可取消的后台任务
///
/// 闭包接收 `&AtomicBool` 取消标志，外部调用 `cancel()` 或
/// 任务被丢弃时自动发送取消信号。
pub struct CancellableTask<T> {
    /// 结果接收端
    rx: mpsc::Receiver<T>,
    /// 取消标志（Drop 时自动置 true）
    cancel: Arc<AtomicBool>,
}

#[allow(dead_code)]
impl<T: Send + 'static> CancellableTask<T> {
    /// 在后台线程执行闭包（带取消标志），返回任务句柄
    pub fn spawn(f: impl FnOnce(&AtomicBool) -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f(&c));
        });
        Self { rx, cancel }
    }

    /// 发送取消信号（后台闭包可通过 `AtomicBool` 检测）
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl<T> Pollable<T> for CancellableTask<T> {
    fn rx(&self) -> &mpsc::Receiver<T> {
        &self.rx
    }
}

impl<T> Drop for CancellableTask<T> {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
