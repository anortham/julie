class Base {
public:
    int id() const {
        return 1;
    }
};

class Worker : public Base {
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

int helper_value(int value) {
    return value + 2;
}

int run_worker() {
    return helper_value(20);
}
