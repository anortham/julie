export enum DatabaseDriver {
  PostgreSQL = 'postgresql',
  MySQL = 'mysql',
  SQLite = 'sqlite',
  MongoDB = 'mongodb'
}

export type QueryOptions<T> = {
  where?: Partial<T>;
  orderBy?: keyof T;
  limit?: number;
  offset?: number;
};

export interface Entity {
  id: string;
  createdAt: Date;
  updatedAt?: Date;
}

export abstract class BaseRepository<T extends Entity> {
  protected abstract tableName: string;
  protected driver: DatabaseDriver;

  constructor(driver: DatabaseDriver = DatabaseDriver.PostgreSQL) {
    this.driver = driver;
  }

  abstract findById(id: string): Promise<T | null>;
  abstract findMany(options?: QueryOptions<T>): Promise<T[]>;
  abstract create(entity: Omit<T, 'id' | 'createdAt'>): Promise<T>;
  abstract update(id: string, updates: Partial<T>): Promise<T | null>;
  abstract delete(id: string): Promise<boolean>;

  protected buildQuery<K extends keyof T>(options: QueryOptions<T>): string {
    let query = `SELECT * FROM ${this.tableName}`;

    if (options.where) {
      const conditions = Object.entries(options.where)
        .map(([key, value]) => `${key} = '${value}'`)
        .join(' AND ');
      query += ` WHERE ${conditions}`;
    }

    if (options.orderBy) {
      query += ` ORDER BY ${String(options.orderBy)}`;
    }

    if (options.limit) {
      query += ` LIMIT ${options.limit}`;
    }

    if (options.offset) {
      query += ` OFFSET ${options.offset}`;
    }

    return query;
  }
}

export class CacheManager<K, V> {
  private cache = new Map<K, V>();
  private maxSize: number;

  constructor(maxSize: number = 1000) {
    this.maxSize = maxSize;
  }

  get(key: K): V | undefined {
    return this.cache.get(key);
  }

  set(key: K, value: V): void {
    if (this.cache.size >= this.maxSize) {
      const firstKey = this.cache.keys().next().value;
      this.cache.delete(firstKey);
    }
    this.cache.set(key, value);
  }

  has(key: K): boolean {
    return this.cache.has(key);
  }

  clear(): void {
    this.cache.clear();
  }

  size(): number {
    return this.cache.size;
  }
}

export namespace ValidationUtils {
  export function isRequired<T>(value: T | null | undefined): value is T {
    return value !== null && value !== undefined;
  }

  export function isEmail(value: string): boolean {
    return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value);
  }

  export function isInRange(value: number, min: number, max: number): boolean {
    return value >= min && value <= max;
  }

  export function validateEntity<T extends Entity>(entity: T): ValidationResult {
    const errors: string[] = [];

    if (!isRequired(entity.id)) {
      errors.push('ID is required');
    }

    if (!isRequired(entity.createdAt)) {
      errors.push('Created date is required');
    }

    return {
      isValid: errors.length === 0,
      errors
    };
  }
}

export interface ValidationResult {
  isValid: boolean;
  errors: string[];
}

export function asyncRetry<T>(
  fn: () => Promise<T>,
  maxAttempts: number = 3,
  delayMs: number = 1000
): Promise<T> {
  return new Promise(async (resolve, reject) => {
    let lastError: Error;

    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      try {
        const result = await fn();
        resolve(result);
        return;
      } catch (error) {
        lastError = error as Error;

        if (attempt === maxAttempts) {
          reject(lastError);
          return;
        }

        await new Promise(resolve => setTimeout(resolve, delayMs * attempt));
      }
    }
  });
}

export const CONFIG = {
  database: {
    maxConnections: 10,
    timeout: 30000,
    retryAttempts: 3
  },
  cache: {
    defaultTtl: 300000,
    maxSize: 1000
  }
} as const;