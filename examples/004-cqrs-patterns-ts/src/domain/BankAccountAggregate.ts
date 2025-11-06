import { randomUUID } from "crypto";
import {
  Aggregate,
  AggregateRoot,
  CommandHandler,
  EventSourcingHandler,
} from "@event-sourcing-platform/typescript";
import { AccountOpenedEvent } from "./events/AccountOpenedEvent";
import { MoneyDepositedEvent } from "./events/MoneyDepositedEvent";
import { MoneyWithdrawnEvent } from "./events/MoneyWithdrawnEvent";
import { AccountClosedEvent } from "./events/AccountClosedEvent";
import { OpenAccountCommand } from "./commands/OpenAccountCommand";
import { DepositMoneyCommand } from "./commands/DepositMoneyCommand";
import { WithdrawMoneyCommand } from "./commands/WithdrawMoneyCommand";
import { CloseAccountCommand } from "./commands/CloseAccountCommand";

type AccountEvent =
  | AccountOpenedEvent
  | MoneyDepositedEvent
  | MoneyWithdrawnEvent
  | AccountClosedEvent;

enum AccountStatus {
  Open = "Open",
  Closed = "Closed",
}

@Aggregate("BankAccount")
export class BankAccountAggregate extends AggregateRoot<AccountEvent> {
  private customerId: string = "";
  private accountType: string = "";
  private balance: number = 0;
  private status: AccountStatus = AccountStatus.Open;

  getAggregateType(): string {
    return "BankAccount";
  }

  // ============================================================================
  // COMMAND HANDLERS (Business Logic)
  // ============================================================================

  @CommandHandler("OpenAccountCommand")
  openAccount(command: OpenAccountCommand): void {
    if (this.id) {
      throw new Error("Account already opened");
    }
    if (command.initialBalance < 0) {
      throw new Error("Initial balance cannot be negative");
    }

    this.initialize(command.aggregateId);
    this.apply(
      new AccountOpenedEvent(
        command.aggregateId,
        command.customerId,
        command.accountType,
        command.initialBalance,
      ),
    );
  }

  @CommandHandler("DepositMoneyCommand")
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

    this.apply(
      new MoneyDepositedEvent(
        command.amount,
        command.description,
        randomUUID(),
      ),
    );
  }

  @CommandHandler("WithdrawMoneyCommand")
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

    this.apply(
      new MoneyWithdrawnEvent(command.amount, command.description, randomUUID()),
    );
  }

  @CommandHandler("CloseAccountCommand")
  closeAccount(command: CloseAccountCommand): void {
    if (!this.id) {
      throw new Error("Account not opened");
    }
    if (this.status === AccountStatus.Closed) {
      throw new Error("Account already closed");
    }

    this.apply(new AccountClosedEvent(command.reason, this.balance));
  }

  // ============================================================================
  // EVENT SOURCING HANDLERS (State Reconstruction)
  // ============================================================================

  @EventSourcingHandler("AccountOpened")
  private onAccountOpened(event: AccountOpenedEvent): void {
    this.customerId = event.customerId;
    this.accountType = event.accountType;
    this.balance = event.initialBalance;
    this.status = AccountStatus.Open;
  }

  @EventSourcingHandler("MoneyDeposited")
  private onMoneyDeposited(event: MoneyDepositedEvent): void {
    this.balance += event.amount;
  }

  @EventSourcingHandler("MoneyWithdrawn")
  private onMoneyWithdrawn(event: MoneyWithdrawnEvent): void {
    this.balance -= event.amount;
  }

  @EventSourcingHandler("AccountClosed")
  private onAccountClosed(): void {
    this.status = AccountStatus.Closed;
  }

  // ============================================================================
  // PUBLIC STATE ACCESSORS (For External Inspection)
  // ============================================================================

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

