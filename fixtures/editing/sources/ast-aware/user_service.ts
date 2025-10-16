// Source file for AST-aware rename test
// This test proves Julie uses tree-sitter AST, not regex!

class UserService {
    // UserService implementation
    private data: string;

    constructor() {
        this.data = "UserService"; // String literal - should NOT be renamed!
    }

    getName(): string {
        // Return the name "UserService" as a string
        return "UserService"; // Another string - should NOT be renamed!
    }

    process(): void {
        const service: UserService = this; // Type annotation - SHOULD be renamed
        console.log(service);
    }
}

// UserService is mentioned in this comment - should NOT be renamed!
const myService = new UserService(); // This SHOULD be renamed
myService.process();
