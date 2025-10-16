use super::parse_cpp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_threading_async_and_synchronization_primitives() {
        let cpp_code = r#"
    #include <thread>
    #include <mutex>
    #include <condition_variable>
    #include <future>
    #include <atomic>

    class ThreadSafeCounter {
    public:
        void increment() {
            std::lock_guard<std::mutex> lock(mutex_);
            ++count_;
        }

        int get() const {
            std::lock_guard<std::mutex> lock(mutex_);
            return count_;
        }

        void wait_for_condition() {
            std::unique_lock<std::mutex> lock(mutex_);
            cv_.wait(lock, [this] { return count_ > 10; });
        }

    private:
        mutable std::mutex mutex_;
        std::condition_variable cv_;
        int count_ = 0;
    };

    class AtomicOperations {
    public:
        void atomic_ops() {
            counter_.store(42, std::memory_order_release);
            int value = counter_.load(std::memory_order_acquire);
            counter_.fetch_add(1, std::memory_order_acq_rel);
        }

    private:
        std::atomic<int> counter_{0};
        std::atomic_flag flag_ = ATOMIC_FLAG_INIT;
    };

    class AsyncOperations {
    public:
        std::future<int> async_computation() {
            return std::async(std::launch::async, []() {
                std::this_thread::sleep_for(std::chrono::milliseconds(100));
                return 42;
            });
        }

        void promise_example() {
            std::promise<std::string> promise;
            auto future = promise.get_future();

            std::thread worker([&promise]() {
                promise.set_value("Hello from thread!");
            });

            worker.join();
        }
    };

    // Thread-local storage
    thread_local int tls_counter = 0;

    // Memory ordering
    std::atomic<bool> ready{false};
    std::atomic<int> data{0};

    void producer() {
        data.store(42, std::memory_order_relaxed);
        ready.store(true, std::memory_order_release);
    }
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let thread_safe_counter = symbols.iter().find(|s| s.name == "ThreadSafeCounter");
        assert!(thread_safe_counter.is_some());

        let increment_method = symbols.iter().find(|s| s.name == "increment");
        assert!(increment_method.is_some());

        let atomic_ops_class = symbols.iter().find(|s| s.name == "AtomicOperations");
        assert!(atomic_ops_class.is_some());

        let async_computation = symbols.iter().find(|s| s.name == "async_computation");
        assert!(async_computation.is_some());
        assert!(async_computation
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("std::future<int>"));

        let tls_counter = symbols.iter().find(|s| s.name == "tls_counter");
        assert!(tls_counter.is_some());
        assert!(tls_counter
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("thread_local"));

        let producer = symbols.iter().find(|s| s.name == "producer");
        assert!(producer.is_some());
    }
}
