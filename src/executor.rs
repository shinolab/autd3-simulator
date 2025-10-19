use std::future::Future;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake};

struct Waker {
    condvar: Condvar,
    mutex: Mutex<bool>,
}

impl Waker {
    fn new() -> Self {
        Self {
            condvar: Condvar::new(),
            mutex: Mutex::new(false),
        }
    }

    fn wait(&self) {
        let mut notified = self.mutex.lock().unwrap();
        while !*notified {
            notified = self.condvar.wait(notified).unwrap();
        }
        *notified = false;
    }
}

impl Wake for Waker {
    fn wake_by_ref(self: &Arc<Self>) {
        let mut notified = self.mutex.lock().unwrap();
        *notified = true;
        self.condvar.notify_one();
    }

    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
}

pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);

    let waker_impl = Arc::new(Waker::new());
    let waker = std::task::Waker::from(waker_impl.clone());
    let mut context = Context::from_waker(&waker);

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => {
                waker_impl.wait();
            }
        }
    }
}
