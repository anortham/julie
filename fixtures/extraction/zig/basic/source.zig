pub const Worker = struct {
    id: i32,

    pub fn run(self: Worker) i32 {
        return helper(self.id);
    }
};

pub fn helper(value: i32) i32 {
    return value + 1;
}

pub fn runWorker(worker: Worker) i32 {
    return helper(worker.id);
}
