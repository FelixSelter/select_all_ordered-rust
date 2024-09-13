# Ordered Parallel Future Processing Library

[![Documentation](https://docs.rs/select_all_ordered/badge.svg)](https://docs.rs/select_all_ordered/)

This minimal rust library provides utilities to process asynchronous futures in parallel while ensuring that the results are returned in the order that the futures were provided, regardless of when they complete.

It is the ordered equivalent of https://docs.rs/futures/latest/futures/future/fn.select_all.html

The library includes two main functions:

- `select_all_ordered`: Executes multiple futures in parallel, returning the results in the order they were submitted. It allows you to control the maximum number of futures to process concurrently.
- `select_all_ordered_stream`: Returns an asynchronous stream that yields future results in the order they were provided, processing futures in parallel and providing control over the maximum concurrency.

Please take a look at the examples folder for brief examples
