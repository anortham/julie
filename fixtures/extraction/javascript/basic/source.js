export class Worker {
    constructor(id) {
        this.id = id;
    }

    run() {
        return helper(this.id);
    }
}

function helper(value) {
    return value + 1;
}
