export class RegisterCustomerCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly email: string,
    public readonly name: string,
    public readonly address: string
  ) {}
}

