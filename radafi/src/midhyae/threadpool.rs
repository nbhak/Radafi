use log::{debug, info};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

type Task = Box<dyn FnOnce() + Send + 'static>;

enum Message {
    NewTask(Task),
    Terminate,
}

/**
 * Implements basic threadpool functionality, executing a specified number
 * of tasks concurrently.
 */
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Message>,
}

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        ThreadPool { workers, sender }
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let task = Box::new(f);
        self.sender.send(Message::NewTask(task)).unwrap();
    }

    pub fn terminate(&self) {
        for _ in &self.workers {
            self.sender.send(Message::Terminate).unwrap();
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.terminate();

        for worker in &mut self.workers {
            debug!("Shutting down worker {}", worker.id);
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

/**
 * Implements a worker to execute a task in a threadpool.
 */
struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Message>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            let message = receiver.lock().unwrap().recv().unwrap();

            match message {
                Message::NewTask(task) => {
                    debug!("Worker {} got a task; executing.", id);
                    task();
                }
                Message::Terminate => {
                    info!("Worker {} was told to terminate.", id);
                    break;
                }
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}