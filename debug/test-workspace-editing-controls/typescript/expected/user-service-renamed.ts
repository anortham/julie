import { Logger } from './logger';

export interface User {
  id: string;
  name: string;
  email: string;
  createdAt: Date;
  roles: Role[];
}

export interface Role {
  id: string;
  name: string;
  permissions: Permission[];
}

export interface Permission {
  action: string;
  resource: string;
}

export class AccountService {
  private logger: Logger;
  private cache = new Map<string, User>();

  constructor(logger: Logger) {
    this.logger = logger;
  }

  async findUserById(id: string): Promise<User | null> {
    this.logger.info(`Finding user with ID: ${id}`);

    // Check cache first
    if (this.cache.has(id)) {
      return this.cache.get(id)!;
    }

    try {
      const user = await this.fetchUserFromDatabase(id);
      if (user) {
        this.cache.set(id, user);
      }
      return user;
    } catch (error) {
      this.logger.error(`Failed to find user: ${error}`);
      return null;
    }
  }

  async createUser(userData: Omit<User, 'id' | 'createdAt'>): Promise<User> {
    const user: User = {
      id: this.generateId(),
      createdAt: new Date(),
      ...userData
    };

    await this.saveUserToDatabase(user);
    this.cache.set(user.id, user);

    this.logger.info(`Created user: ${user.name}`);
    return user;
  }

  async updateUser(id: string, updates: Partial<User>): Promise<User | null> {
    const existingUser = await this.findUserById(id);
    if (!existingUser) {
      return null;
    }

    const updatedUser = { ...existingUser, ...updates };
    await this.saveUserToDatabase(updatedUser);
    this.cache.set(id, updatedUser);

    return updatedUser;
  }

  hasPermission(user: User, action: string, resource: string): boolean {
    return user.roles.some(role =>
      role.permissions.some(permission =>
        permission.action === action && permission.resource === resource
      )
    );
  }

  private async fetchUserFromDatabase(id: string): Promise<User | null> {
    // Simulated database call
    await this.delay(100);
    return null; // Placeholder
  }

  private async saveUserToDatabase(user: User): Promise<void> {
    // Simulated database save
    await this.delay(150);
  }

  private generateId(): string {
    return Math.random().toString(36).substring(2, 15);
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

export function validateEmail(email: string): boolean {
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return emailRegex.test(email);
}

export const DEFAULT_ROLES: Role[] = [
  {
    id: 'user',
    name: 'Standard User',
    permissions: [
      { action: 'read', resource: 'profile' },
      { action: 'update', resource: 'profile' }
    ]
  }
];