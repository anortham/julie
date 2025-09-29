// Advanced edge cases for AST-aware refactoring
// Tests complex scenarios where naive replacement would cause catastrophic failures

export class AccountService {
    // Edge case 1: Nested class with same name
    static AccountService = class {
        name = "UserService";
    };

    // Edge case 2: Property with similar name
    userServiceConfig = {
        name: "UserService configuration",
        type: "UserService"
    };

    // Edge case 3: Method parameter named similarly
    initializeUserService(userService: AccountService) {
        return userService;
    }

    // Edge case 4: Generic constraints
    processUsers<T extends AccountService>(users: T[]): T[] {
        return users;
    }

    // Edge case 5: Union types
    getService(): AccountService | OtherService {
        return new AccountService();
    }

    // Edge case 6: Destructuring
    handleUser({ userService }: { userService: AccountService }) {
        return userService;
    }
}

// Edge case 7: Interface extending
interface ExtendedUserService extends AccountService {
    extendedMethod(): void;
}

// Edge case 8: Type alias
type UserServiceType = AccountService;

// Edge case 9: Namespace collision
namespace Internal {
    export class AccountService {
        // This should NOT be renamed
    }
}

// Edge case 10: Import/export aliases
export { AccountService as UserServiceExport };

class OtherService {}