/**
 * Type definitions for VSA Manifest
 *
 * This defines the contract between vsa-cli and vsa-visualizer.
 *
 * Schema version: 2.3.0 (supersedes 2.2.0, 2.1.0, 2.0.0, 1.0.0)
 *
 * BREAKING CHANGES since 1.0.0:
 * - Manifest: Renamed `contexts` field to `bounded_contexts` for clarity
 *
 * NEW FEATURES since 2.2.0:
 * - Aggregate: Added `entities` field (entity objects with identity)
 * - Aggregate: Added `value_objects` field (value objects within aggregate)
 * - Aggregate: Added `folder_name` field (aggregate_* folder name)
 * - Aggregate: Added `context` field (bounded context this aggregate belongs to)
 * - AggregateEntity: New interface for entity metadata within aggregates
 * - AggregateValueObject: New interface for value object metadata within aggregates
 *
 * NEW FEATURES since 2.1.0:
 * - Feature: Added `slice_type` field (command, query, mixed, unknown)
 * - BoundedContext: Added `context_type` field (bounded_context, invalid_module)
 * - BoundedContext: Added `aggregate_count` field
 *
 * NEW FEATURES since 2.0.0:
 * - DomainManifest: Added optional `projections` field (CQRS read models)
 * - Relationships: Added optional `event_to_projections` mapping
 * - Relationships: Added optional `projection_to_read_model` mapping
 * - Projection: New interface for CQRS projection metadata
 *
 * NEW FEATURES since 1.0.0:
 * - DomainManifest: Added optional `value_objects` field
 * - BoundedContext: Restructured type to model features/slices
 * - BoundedContext: Added optional `infrastructure_folders` field
 *
 * When upgrading from schema 1.0.0 to 2.0.0, ensure producers and
 * consumers of the manifest are updated to use the new `bounded_contexts`
 * field name instead of `contexts`.
 */

export interface Manifest {
  version: string;
  schema_version: string;
  generated_at: string;
  domain?: DomainManifest;
  bounded_contexts?: BoundedContext[];
}

export interface DomainManifest {
  aggregates: Aggregate[];
  commands: Command[];
  events: Event[];
  queries?: Query[];
  projections?: Projection[]; // NEW in v2.1.0
  upcasters?: Upcaster[];
  value_objects?: ValueObject[]; // NEW in v2.0.0
  relationships: Relationships;
}

export interface Aggregate {
  name: string;
  file_path: string;
  line_count: number;
  command_handlers: CommandHandler[];
  event_handlers: EventHandler[];
  entities?: AggregateEntity[];       // NEW in v2.3.0 - entities within this aggregate
  value_objects?: AggregateValueObject[]; // NEW in v2.3.0 - value objects within this aggregate
  folder_name?: string;               // NEW in v2.3.0 - aggregate_* folder name if using DDD convention
  context?: string;                   // NEW in v2.3.0 - bounded context this aggregate belongs to
}

/**
 * Entity within an aggregate (has identity) - NEW in v2.3.0
 */
export interface AggregateEntity {
  name: string;
  identity_field?: string;  // e.g., "isolation_id"
  file_path: string;
  line_count: number;
}

/**
 * Value object within an aggregate (immutable, no identity) - NEW in v2.3.0
 */
export interface AggregateValueObject {
  name: string;
  file_path: string;
  is_immutable: boolean;
  line_count: number;
}

export interface CommandHandler {
  command_type: string;
  method_name: string;
  line_number: number;
  emits_events: string[];
}

export interface EventHandler {
  event_type: string;
  method_name: string;
  line_number: number;
}

export interface Command {
  name: string;
  file_path: string;
  target_aggregate?: string;
  has_aggregate_id: boolean;
  fields: Field[];
}

export interface Event {
  name: string;
  event_type: string;
  version: string;
  file_path: string;
  emitted_by: string[];
  handled_by: string[];
  fields: Field[];
  decorator_present: boolean;
}

export interface Field {
  name: string;
  field_type: string;
  required: boolean;
  line_number: number;
}

export interface Query {
  name: string;
  file_path: string;
  fields: Field[];
}

/**
 * Projection metadata (read model builder) - NEW in v2.1.0
 *
 * Projections are read-side components in CQRS that build read models from events.
 * They typically live in query slices and subscribe to domain events.
 */
export interface Projection {
  name: string;
  file_path: string;
  subscribed_events: string[];
  read_model?: string;
  context?: string;
  line_count?: number;
}

export interface Upcaster {
  name: string;
  file_path: string;
  from_version: string;
  to_version: string;
  event_type: string;
}

/**
 * Value Object metadata (NEW in v2.0.0)
 */
export interface ValueObject {
  name: string;
  file_path: string;
  fields: ValueObjectField[];
  is_immutable: boolean;
  line_count: number;
}

export interface ValueObjectField {
  name: string;
  field_type?: string;
  is_optional: boolean;
}

export interface Relationships {
  command_to_aggregate: Record<string, string>;
  aggregate_to_events: Record<string, string[]>;
  event_to_handlers: Record<string, string[]>;
  event_to_projections?: Record<string, string[]>; // NEW in v2.1.0
  projection_to_read_model?: Record<string, string>; // NEW in v2.1.0
}

/**
 * Slice type - classifies whether a feature/slice is command or query
 * NEW in v2.2.0
 */
export type SliceType = 'command' | 'query' | 'mixed' | 'unknown';

/**
 * Context type - classifies whether a directory is a valid bounded context
 * NEW in v2.2.0
 *
 * - bounded_context: Has at least one aggregate (valid DDD bounded context)
 * - invalid_module: No aggregates found (e.g., orphan projection modules)
 */
export type ContextType = 'bounded_context' | 'invalid_module';

/**
 * Bounded Context with features/slices
 */
export interface BoundedContext {
  name: string;
  path: string;
  features: Feature[];
  infrastructure_folders?: string[]; // NEW in v2.0.0
  context_type?: ContextType;        // NEW in v2.2.0 - bounded_context or invalid_module
  aggregate_count?: number;          // NEW in v2.2.0 - number of aggregates in context
}

export interface Feature {
  name: string;
  path: string;
  files: string[];
  slice_type?: SliceType;  // NEW in v2.2.0 - command, query, mixed, or unknown
}

/**
 * Validation error for manifest parsing
 */
export class ManifestValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ManifestValidationError';
  }
}
