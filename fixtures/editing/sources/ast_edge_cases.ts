// Advanced edge cases for AST-aware refactoring
// Tests complex scenarios where naive replacement would cause catastrophic failures

export class UserService {
    // Edge case 1: Nested class with same name
    static UserService = class {
        name = "UserService";
    };

    // Edge case 2: Property with similar name
    userServiceConfig = {
        name: "UserService configuration",
        type: "UserService"
    };

    // Edge case 3: Method parameter named similarly
    initializeUserService(userService: UserService) {
        return userService;
    }

    // Edge case 4: Generic constraints
    processUsers<T extends UserService>(users: T[]): T[] {
        return users;
    }

    // Edge case 5: Union types
    getService(): UserService | OtherService {
        return new UserService();
    }

    // Edge case 6: Destructuring
    handleUser({ userService }: { userService: UserService }) {
        return userService;
    }
}

// Edge case 7: Interface extending
interface ExtendedUserService extends UserService {
    extendedMethod(): void;
}

// Edge case 8: Type alias
type UserServiceType = UserService;

// Edge case 9: Namespace collision
namespace Internal {
    export class UserService {
        // This should NOT be renamed
    }
}

// Edge case 10: Import/export aliases
export { UserService as UserServiceExport };

class OtherService {}