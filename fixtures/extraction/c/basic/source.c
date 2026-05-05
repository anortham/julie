typedef struct Worker {
    int id;
} Worker;

int helper(int value);

int worker_run(Worker *worker) {
    return helper(worker->id);
}

int helper(int value) {
    return value + 1;
}
