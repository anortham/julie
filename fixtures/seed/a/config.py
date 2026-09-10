DEFAULT_TIMEOUT_SECONDS = 30


def load_config(path):
    with open(path) as handle:
        return handle.read()
