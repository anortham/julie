use super::{SymbolKind, parse_cpp};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_exception_handling_and_raii_patterns() {
        let cpp_code = r#"
    #include <exception>
    #include <stdexcept>
    #include <memory>

    // Custom exception class
    class DatabaseException : public std::runtime_error {
    public:
        explicit DatabaseException(const std::string& msg)
            : std::runtime_error(msg), error_code_(0) {}

        DatabaseException(const std::string& msg, int code)
            : std::runtime_error(msg), error_code_(code) {}

        int error_code() const noexcept { return error_code_; }

    private:
        int error_code_;
    };

    // RAII wrapper for file handling
    class FileGuard {
    public:
        explicit FileGuard(const std::string& filename)
            : file_(std::fopen(filename.c_str(), "r")) {
            if (!file_) {
                throw std::runtime_error("Failed to open file: " + filename);
            }
        }

        ~FileGuard() noexcept {
            if (file_) {
                std::fclose(file_);
            }
        }

        // Non-copyable
        FileGuard(const FileGuard&) = delete;
        FileGuard& operator=(const FileGuard&) = delete;

        // Movable
        FileGuard(FileGuard&& other) noexcept : file_(other.file_) {
            other.file_ = nullptr;
        }

        FileGuard& operator=(FileGuard&& other) noexcept {
            if (this != &other) {
                if (file_) std::fclose(file_);
                file_ = other.file_;
                other.file_ = nullptr;
            }
            return *this;
        }

        FILE* get() const noexcept { return file_; }

    private:
        FILE* file_;
    };

    class ExceptionSafetyDemo {
    public:
        void strong_guarantee() try {
            // All operations succeed or all fail
            auto backup = data_;
            data_.clear();
            data_ = process_data();
        } catch (...) {
            // Restore state on exception
            throw;
        }

        void no_throw_swap(ExceptionSafetyDemo& other) noexcept {
            using std::swap;
            swap(data_, other.data_);
        }

    private:
        std::vector<int> data_;

        std::vector<int> process_data() {
            // Simulate processing that might throw
            if (data_.empty()) {
                throw DatabaseException("No data to process");
            }
            return data_;
        }
    };

    // Exception specification (deprecated but still used)
    void legacy_function() throw(std::bad_alloc, DatabaseException);

    // Modern exception specification
    void modern_function() noexcept;
    void maybe_throws() noexcept(false);
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let db_exception = symbols.iter().find(|s| s.name == "DatabaseException");
        assert!(db_exception.is_some());
        assert!(
            db_exception
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("public std::runtime_error")
        );

        let file_guard = symbols.iter().find(|s| s.name == "FileGuard");
        assert!(file_guard.is_some());

        let file_guard_ctor = symbols
            .iter()
            .find(|s| s.name == "FileGuard" && s.kind == SymbolKind::Constructor);
        assert!(file_guard_ctor.is_some());

        let file_guard_dtor = symbols.iter().find(|s| s.name == "~FileGuard");
        assert!(file_guard_dtor.is_some());
        assert!(
            file_guard_dtor
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("noexcept")
        );

        let strong_guarantee = symbols.iter().find(|s| s.name == "strong_guarantee");
        assert!(strong_guarantee.is_some());

        let modern_function = symbols.iter().find(|s| s.name == "modern_function");
        assert!(modern_function.is_some());
        assert!(
            modern_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("noexcept")
        );
    }
}
