//! # Ordered Parallel Future Processing Library
//!
//! This library provides utilities to process asynchronous futures in parallel while ensuring that
//! the results are returned in the order that the futures were provided. It is particularly useful
//! when you need to process futures in parallel but must preserve the order in which they were
//! submitted, regardless of when they complete.
//!
//! The library includes two main functions:
//! - `select_all_ordered`: Executes multiple futures in parallel, returning the results in the order
//!   they were submitted. It allows you to control the maximum number of futures to process concurrently.
//! - `select_all_ordered_stream`: Returns an asynchronous stream that yields future results in the
//!   order they were provided, processing futures in parallel and providing control over the maximum
//!   concurrency.

use std::{collections::HashMap, fmt::Debug, future::Future, pin::Pin, task::Poll};

use futures::{future::select_all, stream};

/// An internal wrapper around a `Future` that includes an index.
/// Because [futures::future::select_all] returns the remaining futures out of order we need to store the index with it.
/// This cannot be an async fn or an async block because the futures are stored inside a vec and this requires a distinct type
struct WrappingFuture<F>
where
    F: Future,
{
    index: usize,
    future: F,
}
impl<F> Future for WrappingFuture<F>
where
    F: Future + std::marker::Unpin,
{
    type Output = (usize, F::Output);

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        match Pin::new(&mut this.future).poll(cx) {
            Poll::Ready(output) => Poll::Ready((this.index, output)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Basically select_all_ordered but with a less useful function signature that allows to share code with select_all_ordered_stream without unnecessary allocations
/// Attention this will not check the processed buffer if the current index is inside
///
/// # Arguments
/// * `to_process` - An iterator yielding the futures to be processed.
/// * `processed` - A HashMap to store already processed futures.
/// * `processing` - A vector containing the currently processing futures.
/// * `current_index` - How many futures have already been removed from [to_process] in previous calls to this function. Needed to calculate the correct index for processed values that will be stored inside the [processed] HashMap
/// * `max_to_process_in_parallel` - The maximum number of futures to process in parallel.
///
/// # Returns
/// Returns a tuple containing:
/// * The first future's result (of type `T`).
/// * The updated HashMap of processed futures.
/// * The remaining futures still being processed.
/// * The iterator of yet-to-be-started futures.
///
/// # Panics
/// Panics if there are no more futures to process or if the number of parallel futures is zero.
async fn select_all_ordered_helper<T: Debug, F, I>(
    mut to_process: I,
    mut processed: HashMap<usize, T>,
    mut processing: Vec<WrappingFuture<F>>,
    current_index: usize,
    max_to_process_in_parallel: usize,
) -> (T, HashMap<usize, T>, Vec<WrappingFuture<F>>, I)
where
    F: Future<Output = T> + std::marker::Unpin,
    I: Iterator<Item = F>,
{
    debug_assert!(max_to_process_in_parallel >= 1,"If 0 futures can be processed in parallel this would never be able to complete. At least one future must be started");

    // Loop until value found
    loop {
        // Fill the to_process buffer with as many items as possible. Keep track of the index
        if processing.len() == 0 {
            for i in 0..(max_to_process_in_parallel - processing.len()) {
                match to_process.next() {
                    None => break,
                    Some(task) => {
                        processing.push(WrappingFuture {
                            index: current_index + i,
                            future: task,
                        });
                    }
                }
            }
        }
        assert!(
            processing.len() > 0,
            "processing and to_process where both empty. No futures to process have been supplied"
        );

        // Wait for one of them to complete
        let ((index, processed_value), _, remaining) = select_all(processing).await;
        // The order of remaining is not guaranteed. However we do not care if the next future is at index 0 because all of them will be processed in parallel anyways.
        // But this makes checking if its the correct value more difficult. Thats why we store the index inside the future. But we need the WrappingFuture to be a distinct type for the vec to be pushed to. async blocks or functions wont work

        // If it is the current one return it
        if index == current_index {
            break (processed_value, processed, remaining, to_process);
        }
        // else store it in buffer and loop until the first one completed
        else {
            processed.insert(index, processed_value);
            processing = remaining
        }
    }
}

/// Polls the first max_to_process_in_parallel futures in parallel, waits for the first one to complete and returns its value along with
/// all futures that have been completed in the meantime and the remaining futures that have not completed yet
/// Ensures that at all times max_to_process_in_parallel futures are being polled until the first one completes, starting to poll the next futures once others complete
/// The same as [futures::future::select_all] but futures at the beginning of the iterator will be processed first
/// Does not guarantee the order of completion but only the order of return.
/// See [select_all_ordered_stream] to process multiple values efficiently in a loop. This should only be called once because it requires sorting and reallocating data for a nicer return type while the stream caches this data for later iterations
///
/// # Arguments
/// * `to_process` - An iterator yielding the futures to be processed.
/// * `max_to_process_in_parallel` - Maximum number of futures to process in parallel. Should be equal to the number of threads of your executor. If its to large maybe the first futures wont be polled by the executor because it can only poll n futures and it might prefer others.
///
/// # Returns
/// A tuple containing:
/// * The result of the first future
/// * Values and indices of the futures that completed while waiting for the first one.
/// * The iterator yielding the remaining futures
///
/// # Panics
/// Panics if `max_to_process_in_parallel` is set to zero.
pub async fn select_all_ordered<T: Debug, F, I>(
    to_process: I,
    max_to_process_in_parallel: usize,
) -> (T, HashMap<usize, T>, impl Iterator<Item = F>)
where
    F: Future<Output = T> + std::marker::Unpin,
    I: Iterator<Item = F>,
{
    debug_assert!(max_to_process_in_parallel >= 1,"If 0 futures can be processed in parallel this would never be able to complete. At least one future must be started");

    let (value, processed, mut remaining, to_process) = select_all_ordered_helper(
        to_process,
        HashMap::new(),
        Vec::with_capacity(max_to_process_in_parallel),
        0,
        max_to_process_in_parallel,
    )
    .await;
    remaining.sort_by_key(|k| k.index);

    (
        value,
        processed.into_iter().collect(),
        remaining
            .into_iter()
            .map(|wrapped| wrapped.future)
            .chain(to_process),
    )
}

/// Creates an asynchronous stream that processes futures in parallel
/// Streams next method returns the results in the same order as the input Iterator and will process futures at the beginning of the iterator first
/// It does not guarantee the order of completion but the order in which the results are returned by next
///
/// # Arguments
/// * `to_process` - An iterator yielding the futures to be processed.
/// * `max_to_process_in_parallel` - Maximum number of futures to process in parallel.
///
/// # Returns
/// A stream yielding the results of the futures in the order of to_process
///
/// # Panics
/// Panics if `max_to_process_in_parallel` is set to zero.
pub fn select_all_ordered_stream<T: Debug, F, I>(
    to_process: I,
    max_to_process_in_parallel: usize,
) -> impl futures::Stream<Item = T>
where
    F: Future<Output = T> + std::marker::Unpin,
    I: Iterator<Item = F>,
{
    assert!(max_to_process_in_parallel >= 1);

    stream::unfold(
        (
            to_process.peekable(),
            HashMap::new(),
            Vec::with_capacity(max_to_process_in_parallel),
            0,
            max_to_process_in_parallel,
        ),
        |(mut to_process, mut processed, processing, current_index, max_to_process_in_parallel)| async move {
            // Check if current_index is inside processed and return it
            if let Some(value) = processed.remove(&current_index) {
                return Some((
                    value,
                    (
                        to_process,
                        processed,
                        processing,
                        current_index + 1,
                        max_to_process_in_parallel,
                    ),
                ));
            }

            // Check if there is more data or the stream ended
            if processing.len() == 0 && to_process.peek().is_none() {
                debug_assert!(processed.len()==0,"If there are no more futures to be processed but we have not found our value inside processed the stream must have ended and therefore nothing should be inside the processed buffer");
                return None;
            }

            // Get the next value and update the buffers
            let (value, processed, processing, to_process) = select_all_ordered_helper(
                to_process,
                processed,
                processing,
                current_index,
                max_to_process_in_parallel,
            )
            .await;

            Some((
                value,
                (
                    to_process,
                    processed,
                    processing,
                    current_index + 1,
                    max_to_process_in_parallel,
                ),
            ))
        },
    )
}
