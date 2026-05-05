pub mod fixture {
    pub struct Worker {
        pub id: i32,
    }

    impl Worker {
        pub fn run(&self) -> i32 {
            helper(self.id)
        }
    }

    pub fn helper(value: i32) -> i32 {
        value + 1
    }
}
