interface User {
    id: number;
    name: string;
    email: string;
}

class UserManager {
    private users: Map<number, User> = new Map();

    constructor() {
        // Initialize with some test users
        this.users.set(1, {
            id: 1,
            name: "Alice Smith",
            email: "alice@example.com"
        });
    }

    public getUserById(id: number): User | undefined {
        return this.users.get(id);
    }

    public addUser(user: User): boolean {
        if (this.users.has(user.id)) {
            return false;
        }
        this.users.set(user.id, user);
        return true;
    }

    private validateUser(user: User): boolean {
        return user.id > 0 &&
               user.name.length > 0 &&
               user.email.includes('@');
    }
}