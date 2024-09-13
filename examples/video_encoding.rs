use futures::executor::block_on;
use futures::stream::StreamExt;
use futures::{executor::ThreadPool, task::SpawnExt};
use select_all_ordered::select_all_ordered_stream;

struct Frame {
    id: usize,
}

#[derive(Debug)]
struct ProcessedFrame {
    id: usize,
}

fn encode_frame(frame: ProcessedFrame) {
    println!("Encoded frame {}", frame.id);
}

async fn process_frame(frame: Frame) -> ProcessedFrame {
    println!("Processing frame {}", frame.id);
    ProcessedFrame { id: frame.id }
}

fn main() {
    let frames = (0..20).map(|id| Frame { id });

    let thread_pool_size = 4;
    let thread_pool = ThreadPool::builder()
        .pool_size(thread_pool_size)
        .create()
        .unwrap();
    let handles = frames.map(|frame| thread_pool.spawn_with_handle(process_frame(frame)).unwrap());

    let stream = select_all_ordered_stream(handles, thread_pool_size);
    let mut pinned_stream = Box::pin(stream);

    let mut next_id = 0;
    block_on(async move {
        while let Some(processed_frame) = pinned_stream.next().await {
            assert!(processed_frame.id == next_id);
            next_id += 1;
            encode_frame(processed_frame);
        }
    });
}
