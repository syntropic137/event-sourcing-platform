import { CommandBus } from '../../infrastructure/CommandBus';
import { CreateProductCommand } from '../../domain/commands/CreateProductCommand';

/**
 * Thin CLI adapter for creating products.
 * No business logic - just translates CLI input to domain command.
 */
export class CreateProductCli {
  constructor(private commandBus: CommandBus) {}

  async execute(productId: string, name: string, price: number, stock: number): Promise<void> {
    const command = new CreateProductCommand(productId, name, price, stock);
    await this.commandBus.dispatch(command);
    console.log(`Product ${productId} created successfully`);
  }
}

