/**
 * Type definitions for VSA Manifest
 *
 * This defines the contract between vsa-cli and vsa-visualizer.
 *
 * Schema version: 2.1.0 (supersedes 2.0.0, 1.0.0)
 *
 * BREAKING CHANGES since 1.0.0:
 * - Manifest: Renamed `contexts` field to `bounded_contexts` for clarity
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
 * Bounded Context with features/slices
 */
export interface BoundedContext {
  name: string;
  path: string;
  features: Feature[];
  infrastructure_folders?: string[]; // NEW in v2.0.0
}

export interface Feature {
  name: string;
  path: string;
  files: string[];
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
