// Test case for AST-aware symbol renaming
// This file contains tricky cases where naive string replacement would fail

export class UserService {
  private userData: string = "This contains UserService in a string literal";

  constructor() {
    console.log("Creating UserService instance");
    // Comment mentioning UserService should NOT be changed
  }

  getUser() {
    const result = `
      User service: UserService handles user data
      Service type: UserService
    `;
    return result;
  }

  // Method name should NOT be changed even though it contains 'User'
  getUserServiceStatus() {
    return "active";
  }
}

// Function name should be changed
function createUserService() {
  return new UserService();
}

// Variable should be changed
const userService = createUserService();

export { UserService as ExportedUserService };