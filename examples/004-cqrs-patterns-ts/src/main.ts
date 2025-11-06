import { randomUUID } from "crypto";
import {
  EventSerializer,
  EventStoreClient,
  EventStoreClientFactory,
  MemoryEventStoreClient,
} from "@neuralempowerment/event-sourcing-typescript";

// Infrastructure
import { CommandBus } from "./infrastructure/CommandBus";
import { QueryBus, AccountSummary, TransactionHistory } from "./infrastructure/QueryBus";

// Events (for serialization)
import { AccountOpenedEvent } from "./domain/events/AccountOpenedEvent";
import { MoneyDepositedEvent } from "./domain/events/MoneyDepositedEvent";
import { MoneyWithdrawnEvent } from "./domain/events/MoneyWithdrawnEvent";
import { AccountClosedEvent } from "./domain/events/AccountClosedEvent";

// Queries
import { GetAccountSummaryQuery } from "./domain/queries/GetAccountSummaryQuery";
import { GetTransactionHistoryQuery } from "./domain/queries/GetTransactionHistoryQuery";
import { GetAccountsByCustomerQuery } from "./domain/queries/GetAccountsByCustomerQuery";

// Command Slices
import { OpenAccountCli } from "./slices/open-account/OpenAccountCli";
import { DepositMoneyCli } from "./slices/deposit-money/DepositMoneyCli";
import { WithdrawMoneyCli } from "./slices/withdraw-money/WithdrawMoneyCli";
import { CloseAccountCli } from "./slices/close-account/CloseAccountCli";

// Query Slices
import { GetAccountSummaryCli } from "./slices/get-account-summary/GetAccountSummaryCli";
import { AccountSummaryProjection } from "./slices/get-account-summary/AccountSummaryProjection";
import { GetTransactionHistoryCli } from "./slices/get-transaction-history/GetTransactionHistoryCli";
import { TransactionHistoryProjection } from "./slices/get-transaction-history/TransactionHistoryProjection";
import { GetAccountsByCustomerCli } from "./slices/get-accounts-by-customer/GetAccountsByCustomerCli";

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
        "   To use in-memory mode instead: rerun with --memory",
    );
    throw error;
  }
  return client;
}

async function main(): Promise<void> {
  const options = parseOptions();
  const client = await createClient(options);

  // Register events for serialization
  EventSerializer.registerEvent(
    "AccountOpened",
    AccountOpenedEvent as unknown as new () => AccountOpenedEvent,
  );
  EventSerializer.registerEvent(
    "MoneyDeposited",
    MoneyDepositedEvent as unknown as new () => MoneyDepositedEvent,
  );
  EventSerializer.registerEvent(
    "MoneyWithdrawn",
    MoneyWithdrawnEvent as unknown as new () => MoneyWithdrawnEvent,
  );
  EventSerializer.registerEvent(
    "AccountClosed",
    AccountClosedEvent as unknown as new () => AccountClosedEvent,
  );

  // ============================================================================
  // INFRASTRUCTURE SETUP
  // ============================================================================

  const commandBus = new CommandBus(client);
  const queryBus = new QueryBus();

  // Initialize projections (read models)
  const accountSummaryProjection = new AccountSummaryProjection();
  const transactionHistoryProjection = new TransactionHistoryProjection();

  // Register query handlers
  queryBus.registerHandler(
    "GetAccountSummaryQuery",
    {
      handle: async (query: GetAccountSummaryQuery): Promise<AccountSummary | undefined> => {
        return accountSummaryProjection.getAccountSummary(query.accountId);
      },
    },
  );

  queryBus.registerHandler(
    "GetTransactionHistoryQuery",
    {
      handle: async (query: GetTransactionHistoryQuery): Promise<TransactionHistory[] | undefined> => {
        return transactionHistoryProjection.getTransactionHistory(query.accountId);
      },
    },
  );

  queryBus.registerHandler(
    "GetAccountsByCustomerQuery",
    {
      handle: async (query: GetAccountsByCustomerQuery): Promise<AccountSummary[] | undefined> => {
        return accountSummaryProjection.getAccountsByCustomer(query.customerId);
      },
    },
  );

  // ============================================================================
  // COMMAND SLICES (Write Side Adapters)
  // ============================================================================

  const openAccountCli = new OpenAccountCli(commandBus);
  const depositMoneyCli = new DepositMoneyCli(commandBus);
  const withdrawMoneyCli = new WithdrawMoneyCli(commandBus);
  const closeAccountCli = new CloseAccountCli(commandBus);

  // ============================================================================
  // QUERY SLICES (Read Side Adapters)
  // ============================================================================

  const getAccountSummaryCli = new GetAccountSummaryCli(queryBus);
  const getTransactionHistoryCli = new GetTransactionHistoryCli(queryBus);
  const getAccountsByCustomerCli = new GetAccountsByCustomerCli(queryBus);

  try {
    console.log("🏦 CQRS Patterns Example: Banking System");
    console.log("=========================================\n");

    const customerId = `customer-${randomUUID()}`;
    const accountId1 = `account-${randomUUID()}`;
    const accountId2 = `account-${randomUUID()}`;

    // ========================================================================
    // COMMAND SIDE (Write Operations)
    // ========================================================================
    console.log("📝 COMMAND SIDE - Processing Business Operations:");
    console.log("---------------------------------------------------");

    await openAccountCli.handle(accountId1, customerId, "Checking", 1000);
    await openAccountCli.handle(accountId2, customerId, "Savings", 5000);
    await depositMoneyCli.handle(accountId1, 500, "Salary deposit");
    await withdrawMoneyCli.handle(accountId1, 200, "ATM withdrawal");
    await depositMoneyCli.handle(accountId2, 1000, "Bonus deposit");

    // ========================================================================
    // BUILD READ MODELS FROM EVENTS (Projections)
    // ========================================================================
    console.log("\n🔄 BUILDING READ MODELS - Processing Events into Projections:");
    console.log("-------------------------------------------------------------");

    const account1Events = await client.readEvents(`BankAccount-${accountId1}`);
    const account2Events = await client.readEvents(`BankAccount-${accountId2}`);
    const allEvents = [...account1Events, ...account2Events];

    accountSummaryProjection.processEvents(allEvents);
    transactionHistoryProjection.processEvents(allEvents);
    console.log(`📊 Built read models from ${allEvents.length} events\n`);

    // ========================================================================
    // QUERY SIDE (Read Operations)
    // ========================================================================
    console.log("📖 QUERY SIDE - Reading Optimized Views:");
    console.log("-----------------------------------------");

    await getAccountSummaryCli.handle(accountId1);
    await getTransactionHistoryCli.handle(accountId1);
    await getAccountsByCustomerCli.handle(customerId);

    // ========================================================================
    // ADDITIONAL COMMAND (Close Account)
    // ========================================================================
    console.log("\n📝 ADDITIONAL COMMAND:");
    console.log("---------------------");
    await closeAccountCli.handle(accountId2, "Customer request");

    // Update read models with new events
    const newEvents = await client.readEvents(`BankAccount-${accountId2}`);
    accountSummaryProjection.processEvents(newEvents);

    await getAccountSummaryCli.handle(accountId2);

    // ========================================================================
    // SUMMARY
    // ========================================================================
    console.log("\n🎉 CQRS Example completed successfully!");
    console.log("=========================================");
    console.log("💡 This example demonstrates:");
    console.log("   • Command/Query Responsibility Segregation (CQRS)");
    console.log("   • Hexagonal Architecture with VSA slices");
    console.log("   • Separate command and query slices (vertical slices)");
    console.log("   • Read models (projections) built from events");
    console.log("   • Optimized queries on denormalized data");
    console.log("   • Clear separation of write and read concerns");
    console.log("   • CommandBus and QueryBus infrastructure");
    console.log("   • Domain logic isolated in aggregates");
    console.log("   • CLI adapters as thin vertical slices");
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

