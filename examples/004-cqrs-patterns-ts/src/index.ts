import { randomUUID } from "crypto";

import {
  AutoDispatchAggregate,
  BaseDomainEvent,
  EventSourcingHandler,
  EventSerializer,
  EventStoreClient,
  EventStoreClientFactory,
  MemoryEventStoreClient,
  RepositoryFactory,
} from "@event-sourcing-platform/typescript";

type ClientMode = "memory" | "grpc";

type Options = {
  mode: ClientMode;
};

function parseOptions(): Options {
  if (process.argv.includes("--memory")) {
    return { mode: "memory" };
  }
  const envMode = (process.env.EVENT_STORE_MODE ?? "").toLowerCase();
  if (envMode === "memory") {
    return { mode: "memory" };
  }
  return { mode: "grpc" };
}

async function createClient(opts: Options): Promise<EventStoreClient> {
  if (opts.mode === "memory") {
    console.log(
      "🧪 Using in-memory event store client (override via --memory).",
    );
    const client = new MemoryEventStoreClient();
    await client.connect();
    return client;
  }

  const serverAddress = process.env.EVENT_STORE_ADDR ?? "127.0.0.1:50051";
  const tenantId = process.env.EVENT_STORE_TENANT ?? "example-tenant";
  console.log(
    `🛰️  Using gRPC event store at ${serverAddress} (tenant=${tenantId})`,
  );

  const client = EventStoreClientFactory.createGrpcClient({
    serverAddress,
    tenantId,
  });
  try {
    await client.connect();
  } catch (error) {
    console.error(
      "⚠️  Failed to connect to the gRPC event store.\n" +
      "   To start dev infrastructure: make dev-start\n" +
      "   To use in-memory mode instead: rerun with --memory"
    );
    throw error;
  }
  return client;
}

// ============================================================================
// DOMAIN EVENTS
// ============================================================================

class AccountOpened extends BaseDomainEvent {
  readonly eventType = "AccountOpened" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public accountId: string,
    public customerId: string,
    public accountType: string,
    public initialBalance: number,
  ) {
    super();
  }
}

class MoneyDeposited extends BaseDomainEvent {
  readonly eventType = "MoneyDeposited" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public amount: number,
    public description: string,
    public transactionId: string,
  ) {
    super();
  }
}

class MoneyWithdrawn extends BaseDomainEvent {
  readonly eventType = "MoneyWithdrawn" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public amount: number,
    public description: string,
    public transactionId: string,
  ) {
    super();
  }
}

class AccountClosed extends BaseDomainEvent {
  readonly eventType = "AccountClosed" as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public reason: string,
    public finalBalance: number,
  ) {
    super();
  }
}

type AccountEvent = AccountOpened | MoneyDeposited | MoneyWithdrawn | AccountClosed;

// ============================================================================
// COMMANDS (Write Side)
// ============================================================================

interface OpenAccountCommand {
  accountId: string;
  customerId: string;
  accountType: string;
  initialBalance: number;
}

interface DepositMoneyCommand {
  accountId: string;
  amount: number;
  description: string;
}

interface WithdrawMoneyCommand {
  accountId: string;
  amount: number;
  description: string;
}

interface CloseAccountCommand {
  accountId: string;
  reason: string;
}

// ============================================================================
// AGGREGATE (Write Side)
// ============================================================================

enum AccountStatus {
  Open = "Open",
  Closed = "Closed",
}

class BankAccountAggregate extends AutoDispatchAggregate<AccountEvent> {
  private customerId: string = "";
  private accountType: string = "";
  private balance: number = 0;
  private status: AccountStatus = AccountStatus.Open;

  getAggregateType(): string {
    return "BankAccount";
  }

  // Command Handlers
  openAccount(command: OpenAccountCommand): void {
    if (this.id) {
      throw new Error("Account already opened");
    }
    if (command.initialBalance < 0) {
      throw new Error("Initial balance cannot be negative");
    }

    this.initialize(command.accountId);
    this.raiseEvent(new AccountOpened(
      command.accountId,
      command.customerId,
      command.accountType,
      command.initialBalance,
    ));
  }

  depositMoney(command: DepositMoneyCommand): void {
    if (!this.id) {
      throw new Error("Account not opened");
    }
    if (this.status !== AccountStatus.Open) {
      throw new Error("Cannot deposit to closed account");
    }
    if (command.amount <= 0) {
      throw new Error("Deposit amount must be positive");
    }

    this.raiseEvent(new MoneyDeposited(
      command.amount,
      command.description,
      randomUUID(),
    ));
  }

  withdrawMoney(command: WithdrawMoneyCommand): void {
    if (!this.id) {
      throw new Error("Account not opened");
    }
    if (this.status !== AccountStatus.Open) {
      throw new Error("Cannot withdraw from closed account");
    }
    if (command.amount <= 0) {
      throw new Error("Withdrawal amount must be positive");
    }
    if (command.amount > this.balance) {
      throw new Error("Insufficient funds");
    }

    this.raiseEvent(new MoneyWithdrawn(
      command.amount,
      command.description,
      randomUUID(),
    ));
  }

  closeAccount(command: CloseAccountCommand): void {
    if (!this.id) {
      throw new Error("Account not opened");
    }
    if (this.status === AccountStatus.Closed) {
      throw new Error("Account already closed");
    }

    this.raiseEvent(new AccountClosed(command.reason, this.balance));
  }

  // Event Handlers
  @EventSourcingHandler("AccountOpened")
  private onAccountOpened(event: AccountOpened): void {
    this.customerId = event.customerId;
    this.accountType = event.accountType;
    this.balance = event.initialBalance;
    this.status = AccountStatus.Open;
  }

  @EventSourcingHandler("MoneyDeposited")
  private onMoneyDeposited(event: MoneyDeposited): void {
    this.balance += event.amount;
  }

  @EventSourcingHandler("MoneyWithdrawn")
  private onMoneyWithdrawn(event: MoneyWithdrawn): void {
    this.balance -= event.amount;
  }

  @EventSourcingHandler("AccountClosed")
  private onAccountClosed(): void {
    this.status = AccountStatus.Closed;
  }

  // Getters for current state
  getBalance(): number {
    return this.balance;
  }

  getStatus(): AccountStatus {
    return this.status;
  }

  getCustomerId(): string {
    return this.customerId;
  }

  getAccountType(): string {
    return this.accountType;
  }
}

// ============================================================================
// COMMAND HANDLERS (Application Layer)
// ============================================================================

class BankAccountCommandHandler {
  constructor(private repository: any) {}

  async handleOpenAccount(command: OpenAccountCommand): Promise<void> {
    const account = new BankAccountAggregate();
    account.openAccount(command);
    await this.repository.save(account);
  }

  async handleDepositMoney(command: DepositMoneyCommand): Promise<void> {
    const account = await this.repository.load(command.accountId);
    if (!account) {
      throw new Error(`Account ${command.accountId} not found`);
    }
    account.depositMoney(command);
    await this.repository.save(account);
  }

  async handleWithdrawMoney(command: WithdrawMoneyCommand): Promise<void> {
    const account = await this.repository.load(command.accountId);
    if (!account) {
      throw new Error(`Account ${command.accountId} not found`);
    }
    account.withdrawMoney(command);
    await this.repository.save(account);
  }

  async handleCloseAccount(command: CloseAccountCommand): Promise<void> {
    const account = await this.repository.load(command.accountId);
    if (!account) {
      throw new Error(`Account ${command.accountId} not found`);
    }
    account.closeAccount(command);
    await this.repository.save(account);
  }
}

// ============================================================================
// READ MODELS (Query Side)
// ============================================================================

interface AccountSummary {
  accountId: string;
  customerId: string;
  accountType: string;
  balance: number;
  status: string;
  transactionCount: number;
  lastActivity?: Date;
}

interface TransactionHistory {
  transactionId: string;
  accountId: string;
  type: "deposit" | "withdrawal";
  amount: number;
  description: string;
  timestamp: Date;
  balanceAfter: number;
}

// ============================================================================
// QUERY HANDLERS (Read Side)
// ============================================================================

class AccountQueryHandler {
  private accountSummaries = new Map<string, AccountSummary>();
  private transactionHistories = new Map<string, TransactionHistory[]>();

  // This would typically be built from events via projections
  buildReadModelsFromEvents(events: any[]): void {
    for (const envelope of events) {
      const event = envelope.event;
      const metadata = envelope.metadata;
      const accountId = metadata.aggregateId;

      switch (event.eventType) {
        case "AccountOpened":
          this.accountSummaries.set(accountId, {
            accountId,
            customerId: event.customerId,
            accountType: event.accountType,
            balance: event.initialBalance,
            status: "Open",
            transactionCount: 0,
            lastActivity: new Date(metadata.timestamp),
          });
          break;

        case "MoneyDeposited":
          const depositSummary = this.accountSummaries.get(accountId);
          if (depositSummary) {
            depositSummary.balance += event.amount;
            depositSummary.transactionCount++;
            depositSummary.lastActivity = new Date(metadata.timestamp);

            const transactions = this.transactionHistories.get(accountId) || [];
            transactions.push({
              transactionId: event.transactionId,
              accountId,
              type: "deposit",
              amount: event.amount,
              description: event.description,
              timestamp: new Date(metadata.timestamp),
              balanceAfter: depositSummary.balance,
            });
            this.transactionHistories.set(accountId, transactions);
          }
          break;

        case "MoneyWithdrawn":
          const withdrawSummary = this.accountSummaries.get(accountId);
          if (withdrawSummary) {
            withdrawSummary.balance -= event.amount;
            withdrawSummary.transactionCount++;
            withdrawSummary.lastActivity = new Date(metadata.timestamp);

            const transactions = this.transactionHistories.get(accountId) || [];
            transactions.push({
              transactionId: event.transactionId,
              accountId,
              type: "withdrawal",
              amount: event.amount,
              description: event.description,
              timestamp: new Date(metadata.timestamp),
              balanceAfter: withdrawSummary.balance,
            });
            this.transactionHistories.set(accountId, transactions);
          }
          break;

        case "AccountClosed":
          const closeSummary = this.accountSummaries.get(accountId);
          if (closeSummary) {
            closeSummary.status = "Closed";
            closeSummary.lastActivity = new Date(metadata.timestamp);
          }
          break;
      }
    }
  }

  getAccountSummary(accountId: string): AccountSummary | undefined {
    return this.accountSummaries.get(accountId);
  }

  getTransactionHistory(accountId: string): TransactionHistory[] {
    return this.transactionHistories.get(accountId) || [];
  }

  getAllAccountSummaries(): AccountSummary[] {
    return Array.from(this.accountSummaries.values());
  }

  getAccountsByCustomer(customerId: string): AccountSummary[] {
    return Array.from(this.accountSummaries.values())
      .filter(account => account.customerId === customerId);
  }
}

// ============================================================================
// MAIN EXAMPLE
// ============================================================================

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  // Register all event types
  EventSerializer.registerEvent(
    "AccountOpened",
    AccountOpened as unknown as new () => AccountOpened,
  );
  EventSerializer.registerEvent(
    "MoneyDeposited",
    MoneyDeposited as unknown as new () => MoneyDeposited,
  );
  EventSerializer.registerEvent(
    "MoneyWithdrawn",
    MoneyWithdrawn as unknown as new () => MoneyWithdrawn,
  );
  EventSerializer.registerEvent(
    "AccountClosed",
    AccountClosed as unknown as new () => AccountClosed,
  );

  // Set up repositories and handlers
  const repositoryFactory = new RepositoryFactory(client);
  const accountRepository = repositoryFactory.createRepository(
    () => new BankAccountAggregate(),
    "BankAccount",
  );

  const commandHandler = new BankAccountCommandHandler(accountRepository);
  const queryHandler = new AccountQueryHandler();

  try {
    console.log("🏦 CQRS Patterns Example: Banking System");
    console.log("=======================================");

    const customerId = `customer-${randomUUID()}`;
    const accountId1 = `account-${randomUUID()}`;
    const accountId2 = `account-${randomUUID()}`;

    // ========================================================================
    // COMMAND SIDE (Write Operations)
    // ========================================================================
    console.log("\n📝 COMMAND SIDE - Processing Business Operations:");

    // Open first account
    await commandHandler.handleOpenAccount({
      accountId: accountId1,
      customerId,
      accountType: "Checking",
      initialBalance: 1000,
    });
    console.log(`✅ Opened checking account ${accountId1} with $1000`);

    // Open second account
    await commandHandler.handleOpenAccount({
      accountId: accountId2,
      customerId,
      accountType: "Savings",
      initialBalance: 5000,
    });
    console.log(`✅ Opened savings account ${accountId2} with $5000`);

    // Perform some transactions
    await commandHandler.handleDepositMoney({
      accountId: accountId1,
      amount: 500,
      description: "Salary deposit",
    });
    console.log(`💰 Deposited $500 to checking account`);

    await commandHandler.handleWithdrawMoney({
      accountId: accountId1,
      amount: 200,
      description: "ATM withdrawal",
    });
    console.log(`💸 Withdrew $200 from checking account`);

    await commandHandler.handleDepositMoney({
      accountId: accountId2,
      amount: 1000,
      description: "Bonus deposit",
    });
    console.log(`💰 Deposited $1000 to savings account`);

    // ========================================================================
    // BUILD READ MODELS FROM EVENTS
    // ========================================================================
    console.log("\n🔄 BUILDING READ MODELS - Processing Events into Projections:");

    // Read all events for both accounts to build read models
    const account1Events = await client.readEvents(`BankAccount-${accountId1}`);
    const account2Events = await client.readEvents(`BankAccount-${accountId2}`);
    const allEvents = [...account1Events, ...account2Events];

    queryHandler.buildReadModelsFromEvents(allEvents);
    console.log(`📊 Built read models from ${allEvents.length} events`);

    // ========================================================================
    // QUERY SIDE (Read Operations)
    // ========================================================================
    console.log("\n📖 QUERY SIDE - Reading Optimized Views:");

    // Query account summaries
    const account1Summary = queryHandler.getAccountSummary(accountId1);
    const account2Summary = queryHandler.getAccountSummary(accountId2);

    console.log(`\n💳 Account Summaries:`);
    if (account1Summary) {
      console.log(`   Checking: $${account1Summary.balance} (${account1Summary.transactionCount} transactions)`);
    }
    if (account2Summary) {
      console.log(`   Savings: $${account2Summary.balance} (${account2Summary.transactionCount} transactions)`);
    }

    // Query transaction history
    const account1Transactions = queryHandler.getTransactionHistory(accountId1);
    console.log(`\n📋 Checking Account Transaction History:`);
    account1Transactions.forEach((tx, i) => {
      const sign = tx.type === "deposit" ? "+" : "-";
      console.log(`   ${i + 1}. ${sign}$${tx.amount} - ${tx.description} (Balance: $${tx.balanceAfter})`);
    });

    // Query by customer
    const customerAccounts = queryHandler.getAccountsByCustomer(customerId);
    console.log(`\n👤 Customer ${customerId} has ${customerAccounts.length} accounts:`);
    customerAccounts.forEach(account => {
      console.log(`   ${account.accountType}: $${account.balance} (${account.status})`);
    });

    // Close one account
    await commandHandler.handleCloseAccount({
      accountId: accountId2,
      reason: "Customer request",
    });
    console.log(`\n🔒 Closed savings account`);

    // Update read models with new events
    const newEvents = await client.readEvents(`BankAccount-${accountId2}`);
    queryHandler.buildReadModelsFromEvents(newEvents);

    const updatedAccount2Summary = queryHandler.getAccountSummary(accountId2);
    console.log(`💳 Updated Savings Account: $${updatedAccount2Summary?.balance} (${updatedAccount2Summary?.status})`);

    console.log("\n🎉 CQRS Example completed successfully!");
    console.log("💡 This example demonstrates:");
    console.log("   • Command/Query Responsibility Segregation (CQRS)");
    console.log("   • Separate command and query handlers");
    console.log("   • Read models built from events (projections)");
    console.log("   • Optimized queries on denormalized data");
    console.log("   • Clear separation of write and read concerns");

  } finally {
    await client.disconnect();
  }
}

if (require.main === module) {
  main().catch((error) => {
    console.error("❌ Example failed", error);
    process.exitCode = 1;
  });
}
