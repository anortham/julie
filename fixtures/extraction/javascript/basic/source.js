export class Worker {
    constructor(id) {
        this.id = id;
    }

    run() {
        return helper(this.id);
    }
}

/**
 * Increment a worker id.
 * @returns {number}
 */
function helper(value) {
    return value + 1;
}
