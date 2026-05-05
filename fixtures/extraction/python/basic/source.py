class Worker:
    def __init__(self, id: int) -> None:
        self.id = id

    def run(self) -> int:
        return helper(self.id)


def helper(value: int) -> int:
    return value + 1
