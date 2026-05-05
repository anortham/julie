class Worker {
public:
    explicit Worker(int id) : id_(id) {}

    int run() const {
        return helper(id_);
    }

private:
    int helper(int value) const {
        return value + 1;
    }

    int id_;
};
