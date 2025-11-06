export class UpdateCustomerAddressCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly newAddress: string
  ) {}
}

