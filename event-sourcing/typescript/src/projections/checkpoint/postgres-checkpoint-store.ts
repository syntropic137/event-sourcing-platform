/**
 * PostgreSQL checkpoint store implementation
 */

import { ProjectionCheckpoint } from '../types';
import { ProjectionCheckpointStore } from './checkpoint-store';

/**
 * PostgreSQL client interface (minimal subset needed)
 * Compatible with pg, postgres.js, or any similar client
 */
export interface PostgresClient {
  query<T = unknown>(sql: string, params?: unknown[]): Promise<{ rows: T[] }>;
}

/**
 * Options for PostgreSQL checkpoint store
 */
export interface PostgresCheckpointStoreOptions {
  /** Database client */
  client: PostgresClient;

  /** Table name for checkpoints (default: projection_checkpoints) */
  tableName?: string;

  /** Schema name (default: public) */
  schemaName?: string;
}

/**
 * PostgreSQL-backed checkpoint store
 *
 * Stores projection checkpoints in a PostgreSQL table for durability.
 *
 * @example
 * ```typescript
 * import { Pool } from 'pg';
 *
 * const pool = new Pool({ connectionString: process.env.DATABASE_URL });
 * const checkpointStore = new PostgresCheckpointStore({
 *   client: pool,
 *   tableName: 'projection_checkpoints',
 * });
 *
 * // Ensure table exists
 * await checkpointStore.ensureTable();
 * ```
 */
export class PostgresCheckpointStore implements ProjectionCheckpointStore {
  private readonly client: PostgresClient;
  private readonly tableName: string;
  private readonly schemaName: string;
  private readonly fullTableName: string;

  constructor(options: PostgresCheckpointStoreOptions) {
    this.client = options.client;
    this.tableName = options.tableName ?? 'projection_checkpoints';
    this.schemaName = options.schemaName ?? 'public';
    this.fullTableName = `"${this.schemaName}"."${this.tableName}"`;
  }

  /**
   * Create the checkpoints table if it doesn't exist
   */
  async ensureTable(): Promise<void> {
    const sql = `
      CREATE TABLE IF NOT EXISTS ${this.fullTableName} (
        projection_name VARCHAR(255) PRIMARY KEY,
        global_position BIGINT NOT NULL,
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        version INTEGER NOT NULL DEFAULT 1
      );

      CREATE INDEX IF NOT EXISTS idx_${this.tableName}_position
        ON ${this.fullTableName} (global_position);
    `;

    await this.client.query(sql);
  }

  async getCheckpoint(projectionName: string): Promise<ProjectionCheckpoint | null> {
    const sql = `
      SELECT projection_name, global_position, updated_at, version
      FROM ${this.fullTableName}
      WHERE projection_name = $1
    `;

    const result = await this.client.query<{
      projection_name: string;
      global_position: string | number;
      updated_at: Date;
      version: number;
    }>(sql, [projectionName]);

    if (result.rows.length === 0) {
      return null;
    }

    const row = result.rows[0];
    return {
      projectionName: row.projection_name,
      globalPosition: Number(row.global_position),
      updatedAt: new Date(row.updated_at),
      version: row.version,
    };
  }

  async saveCheckpoint(checkpoint: ProjectionCheckpoint): Promise<void> {
    const sql = `
      INSERT INTO ${this.fullTableName} (projection_name, global_position, updated_at, version)
      VALUES ($1, $2, $3, $4)
      ON CONFLICT (projection_name)
      DO UPDATE SET
        global_position = EXCLUDED.global_position,
        updated_at = EXCLUDED.updated_at,
        version = EXCLUDED.version
    `;

    await this.client.query(sql, [
      checkpoint.projectionName,
      checkpoint.globalPosition,
      checkpoint.updatedAt,
      checkpoint.version,
    ]);
  }

  async deleteCheckpoint(projectionName: string): Promise<void> {
    const sql = `DELETE FROM ${this.fullTableName} WHERE projection_name = $1`;
    await this.client.query(sql, [projectionName]);
  }

  async getAllCheckpoints(): Promise<ProjectionCheckpoint[]> {
    const sql = `
      SELECT projection_name, global_position, updated_at, version
      FROM ${this.fullTableName}
      ORDER BY projection_name
    `;

    const result = await this.client.query<{
      projection_name: string;
      global_position: string | number;
      updated_at: Date;
      version: number;
    }>(sql);

    return result.rows.map((row) => ({
      projectionName: row.projection_name,
      globalPosition: Number(row.global_position),
      updatedAt: new Date(row.updated_at),
      version: row.version,
    }));
  }

  async getMinimumPosition(): Promise<number> {
    const sql = `
      SELECT COALESCE(MIN(global_position), 0) as min_position
      FROM ${this.fullTableName}
    `;

    const result = await this.client.query<{ min_position: string | number }>(sql);
    return Number(result.rows[0]?.min_position ?? 0);
  }
}
