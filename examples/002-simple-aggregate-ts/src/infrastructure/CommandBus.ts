import { RepositoryFactory } from "@neuralempowerment/event-sourcing-typescript";
import { OrderAggregate } from "../domain/OrderAggregate";

/**
 * Command Bus
 * 
 * Routes commands to the appropriate aggregate.
 * Part of the infrastructure/application services layer.
 * 
 * Responsibilities:
 * - Load or create aggregate from repository
 * - Dispatch command to aggregate's @CommandHandler
 * - Save aggregate (persists uncommitted events)
 * 
 * Pattern (ADR-004):
 * Slice → CommandBus → Aggregate → Events → Repository
 */
export class CommandBus {
  private repositoryFactory: RepositoryFactory;

  constructor(repositoryFactory: RepositoryFactory) {
    this.repositoryFactory = repositoryFactory;
  }

  /**
   * Send a command to be processed by its aggregate.
   * 
   * @param command - Command with aggregateId property
   */
  async send(command: any): Promise<void> {
    // Create repository for OrderAggregate
    // In a real application, this would route to different aggregates
    // based on command type
    const repository = this.repositoryFactory.createRepository(
      () => new OrderAggregate(),
      "Order",
    );

    // Load existing aggregate or create new one
    let aggregate = await repository.load(command.aggregateId);
    if (!aggregate) {
      aggregate = new OrderAggregate();
    }

    // Dispatch command to aggregate (invokes @CommandHandler method)
    (aggregate as any).handleCommand(command);

    // Save aggregate (persists uncommitted events)
    await repository.save(aggregate);
  }
}

