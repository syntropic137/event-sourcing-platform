/**
 * Type definitions for VSA Manifest
 *
 * This defines the contract between vsa-cli and vsa-visualizer.
 * Schema version: 1.0.0
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
  upcasters?: Upcaster[];
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

export interface Upcaster {
  name: string;
  file_path: string;
  from_version: string;
  to_version: string;
  event_type: string;
}

export interface Relationships {
  command_to_aggregate: Record<string, string>;
  aggregate_to_events: Record<string, string[]>;
  event_to_handlers: Record<string, string[]>;
}

export interface BoundedContext {
  name: string;
  description?: string;
  aggregates: string[];
  publishes: string[];
  subscribes: string[];
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
