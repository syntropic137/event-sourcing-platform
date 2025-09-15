/**
 * Example 001: Basic Event Store Usage
 * 
 * This example demonstrates:
 * - Connecting to the event store
 * - Writing events to streams
 * - Reading events from streams
 * - No event sourcing patterns (just raw event store usage)
 */

import { EventStoreClientFactory, BaseDomainEvent, EventFactory } from '@event-sourcing-platform/typescript';

// Define some basic events
class UserRegistered extends BaseDomainEvent {
  readonly eventType = 'UserRegistered';
  readonly schemaVersion = 1;

  constructor(
    public readonly userId: string,
    public readonly email: string,
    public readonly name: string
  ) {
    super();
  }
}

class UserEmailChanged extends BaseDomainEvent {
  readonly eventType = 'UserEmailChanged';
  readonly schemaVersion = 1;

  constructor(
    public readonly userId: string,
    public readonly oldEmail: string,
    public readonly newEmail: string
  ) {
    super();
  }
}

async function runBasicStoreExample() {
  console.log('🚀 Starting Basic Event Store Example');
  console.log('=====================================');

  // Create event store client
  const eventStoreClient = EventStoreClientFactory.createGrpcClient({
    serverAddress: 'localhost:50051',
    eventStoreUrl: 'grpc://localhost:50051',
    timeoutMs: 5000,
  });

  try {
    // Connect to event store
    console.log('📡 Connecting to event store...');
    await eventStoreClient.connect();
    console.log('✅ Connected to event store');

    const userId = crypto.randomUUID();
    const streamName = `user-${userId}`;

    // Create some events
    const userRegistered = new UserRegistered(userId, 'john@example.com', 'John Doe');
    const emailChanged = new UserEmailChanged(userId, 'john@example.com', 'john.doe@example.com');

    // Create event envelopes
    const registeredEnvelope = EventFactory.create(userRegistered, {
      aggregateId: userId,
      aggregateType: 'User',
      aggregateVersion: 1,
    });

    const emailChangedEnvelope = EventFactory.create(emailChanged, {
      aggregateId: userId,
      aggregateType: 'User',
      aggregateVersion: 2,
    });

    // Write events to stream
    console.log(`📝 Writing events to stream: ${streamName}`);
    await eventStoreClient.appendEvents(streamName, [registeredEnvelope]);
    console.log('✅ Written UserRegistered event');

    await eventStoreClient.appendEvents(streamName, [emailChangedEnvelope], 1);
    console.log('✅ Written UserEmailChanged event');

    // Read events from stream
    console.log(`📖 Reading events from stream: ${streamName}`);
    const events = await eventStoreClient.readEvents(streamName);
    
    console.log(`📋 Found ${events.length} events:`);
    events.forEach((envelope, index) => {
      console.log(`  ${index + 1}. ${envelope.event.eventType} (v${envelope.metadata.aggregateVersion})`);
      console.log(`     Event ID: ${envelope.metadata.eventId}`);
      console.log(`     Timestamp: ${envelope.metadata.timestamp}`);
      console.log(`     Data:`, envelope.event.toJson());
    });

    // Check if stream exists
    console.log(`🔍 Checking if stream exists: ${streamName}`);
    const exists = await eventStoreClient.streamExists(streamName);
    console.log(`✅ Stream exists: ${exists}`);

    // Read events from non-existent stream
    const nonExistentStream = 'non-existent-stream';
    console.log(`📖 Reading from non-existent stream: ${nonExistentStream}`);
    const noEvents = await eventStoreClient.readEvents(nonExistentStream);
    console.log(`📋 Found ${noEvents.length} events (expected 0)`);

  } catch (error) {
    console.error('❌ Error:', error);
  } finally {
    // Disconnect
    console.log('📡 Disconnecting from event store...');
    await eventStoreClient.disconnect();
    console.log('✅ Disconnected');
  }

  console.log('🎉 Basic Event Store Example completed');
}

// Run the example
if (require.main === module) {
  runBasicStoreExample().catch(console.error);
}
