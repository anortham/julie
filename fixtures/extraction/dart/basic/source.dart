abstract class Job {
  int run();
}

class Worker extends Job {
  final int id;

  Worker(this.id);

  @override
  int run() {
    return helper(id);
  }
}

int helper(int value) {
  return value + 1;
}
