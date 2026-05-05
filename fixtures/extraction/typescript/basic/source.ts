export interface Job {
    run(): number;
}

export class Worker implements Job {
    constructor(private id: number) {}

    run(): number {
        return helper(this.id);
    }
}

function helper(value: number): number {
    return value + 1;
}
