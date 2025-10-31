//! Go Concurrency Tests - Channels, Goroutines, and Sync Primitives
//!
//! Tests for extracting Go's concurrency features:
//! - Channel types (bidirectional, send-only, receive-only)
//! - Goroutine patterns (go statements, defer, closures)
//! - Context usage (context.Context, context.CancelFunc)
//! - Sync primitives (sync.WaitGroup, sync.RWMutex, sync.Mutex)
//! - Select statements and channel operations
//! - Worker pool patterns with channels

use crate::extractors::base::SymbolKind;
use crate::extractors::go::GoExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[test]
fn test_handle_channel_types_and_goroutines() {
    let code = r#"
package main

func ProcessData(input <-chan string) chan string {
    output := make(chan string)

    go func() {
        defer close(output)
        for data := range input {
            processed := processItem(data)
            output <- processed
        }
    }()

    return output
}

func SendData(ch chan<- string, data string) {
    ch <- data
}
"#;
    let tree = init_parser(code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string(), &workspace_root);
    let symbols = extractor.extract_symbols(&tree);

    let process_data = symbols.iter().find(|s| s.name == "ProcessData");
    assert!(process_data.is_some());
    let process_data = process_data.unwrap();
    assert!(process_data
        .signature
        .as_ref()
        .unwrap()
        .contains("<-chan string"));
    assert!(process_data
        .signature
        .as_ref()
        .unwrap()
        .contains("chan string"));

    let send_data = symbols.iter().find(|s| s.name == "SendData");
    assert!(send_data.is_some());
    let send_data = send_data.unwrap();
    assert!(send_data
        .signature
        .as_ref()
        .unwrap()
        .contains("chan<- string"));
}

#[test]
fn test_extract_concurrency_primitives_and_patterns() {
    let code = r#"
package main

import (
    "context"
    "sync"
    "time"
)

// WorkerPool represents a pool of workers
type WorkerPool struct {
    workers    int
    jobQueue   chan Job
    resultCh   chan Result
    wg         sync.WaitGroup
    mu         sync.RWMutex
    done       chan struct{}
    ctx        context.Context
    cancel     context.CancelFunc
}

// Job represents work to be done
type Job struct {
    ID   int
    Data interface{}
}

// Result represents the result of a job
type Result struct {
    JobID int
    Data  interface{}
    Error error
}

// NewWorkerPool creates a new worker pool
func NewWorkerPool(workers int, bufferSize int) *WorkerPool {
    ctx, cancel := context.WithCancel(context.Background())
    return &WorkerPool{
        workers:  workers,
        jobQueue: make(chan Job, bufferSize),
        resultCh: make(chan Result, bufferSize),
        done:     make(chan struct{}),
        ctx:      ctx,
        cancel:   cancel,
    }
}

// Start starts the worker pool
func (wp *WorkerPool) Start() {
    for i := 0; i < wp.workers; i++ {
        wp.wg.Add(1)
        go wp.worker(i)
    }
}

// worker is the main worker goroutine
func (wp *WorkerPool) worker(id int) {
    defer wp.wg.Done()

    for {
        select {
        case job := <-wp.jobQueue:
            result := wp.processJob(job)
            select {
            case wp.resultCh <- result:
            case <-wp.ctx.Done():
                return
            }
        case <-wp.ctx.Done():
            return
        }
    }
}
"#;
    let tree = init_parser(code, "go");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        GoExtractor::new("go".to_string(), "test.go".to_string(), code.to_string(), &workspace_root);
    let symbols = extractor.extract_symbols(&tree);

    let worker_pool = symbols.iter().find(|s| s.name == "WorkerPool");
    assert!(worker_pool.is_some());
    let worker_pool = worker_pool.unwrap();
    assert_eq!(worker_pool.kind, SymbolKind::Class);
    assert!(worker_pool
        .signature
        .as_ref()
        .unwrap()
        .contains("type WorkerPool struct"));

    let job = symbols.iter().find(|s| s.name == "Job");
    assert!(job.is_some());
    let job = job.unwrap();
    assert_eq!(job.kind, SymbolKind::Class);

    let result_symbol = symbols.iter().find(|s| s.name == "Result");
    assert!(result_symbol.is_some());
    let result_symbol = result_symbol.unwrap();
    assert_eq!(result_symbol.kind, SymbolKind::Class);

    let new_worker_pool = symbols.iter().find(|s| s.name == "NewWorkerPool");
    assert!(new_worker_pool.is_some());
    let new_worker_pool = new_worker_pool.unwrap();
    assert!(new_worker_pool
        .signature
        .as_ref()
        .unwrap()
        .contains("func NewWorkerPool(workers int, bufferSize int) *WorkerPool"));

    let start_method = symbols.iter().find(|s| s.name == "Start");
    assert!(start_method.is_some());
    let start_method = start_method.unwrap();
    assert!(start_method
        .signature
        .as_ref()
        .unwrap()
        .contains("func (wp *WorkerPool) Start()"));

    let worker_method = symbols.iter().find(|s| s.name == "worker");
    assert!(worker_method.is_some());
    let worker_method = worker_method.unwrap();
    assert!(worker_method
        .signature
        .as_ref()
        .unwrap()
        .contains("func (wp *WorkerPool) worker(id int)"));
}
