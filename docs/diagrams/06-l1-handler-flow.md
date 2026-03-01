# L1 Handler Transaction Flow

## L1 Scraping - Initialization

```mermaid
sequenceDiagram
    participant L1 as Ethereum L1
    participant Scraper as L1 Scraper
    participant Provider as L1 Provider
    participant TxMgr as Transaction Manager

    Note over Scraper: Startup
    Scraper->>Scraper: fetch_start_block()
    Scraper->>L1: get_latest_block()
    L1-->>Scraper: L1BlockReference

    rect rgb(240, 248, 255)
        Note over Scraper,TxMgr: Initialize (once at startup)
        Scraper->>L1: events(start_block..latest, tracked_identifiers)
        L1-->>Scraper: L1 events
        Note over Scraper: Convert to Starknet Events<br/>Calculate L2 tx hashes
        Scraper->>Provider: initialize(historic_l2_height, events)
        Provider->>TxMgr: add_tx() for each L1HandlerTransaction
        Provider-->>Scraper: Ok
    end
```

## L1 Scraping - Continuous Polling

```mermaid
sequenceDiagram
    participant L1 as Ethereum L1
    participant Scraper as L1 Scraper
    participant Provider as L1 Provider
    participant TxMgr as Transaction Manager

    loop Poll for new events
        Scraper->>L1: get_latest_block()
        L1-->>Scraper: latest_block
        Scraper->>Scraper: detect_reorg(last_scraped, latest)

        Scraper->>L1: events(last_scraped..latest, tracked_identifiers)
        L1-->>Scraper: L1 events
        Scraper->>Provider: add_events(events)
        Provider-->>Scraper: Ok
    end
```

## L1 Event Types Handling

```mermaid
sequenceDiagram
    participant Provider as L1 Provider
    participant TxMgr as Transaction Manager

    Note over Provider: For each event received

    alt LogMessageToL2
        Provider->>TxMgr: add_tx(L1HandlerTransaction)
        Note over TxMgr: Add to proposable transactions
    else MessageToL2CancellationStarted
        Provider->>TxMgr: request_cancellation(tx_hash, timestamp)
        Note over TxMgr: Start cancellation timelock
    else MessageToL2Canceled
        Provider->>TxMgr: finalize_cancellation(tx_hash)
        Note over TxMgr: Remove from proposable
    else ConsumedMessageToL2
        Provider->>TxMgr: consume_tx(tx_hash, timestamp)
        Note over TxMgr: Mark as consumed on L1
    end
```

## Block Proposal - Getting L1 Transactions

```mermaid
sequenceDiagram
    participant B as Batcher
    participant TxProv as ProposeTransactionProvider
    participant L1P as L1 Provider
    participant TxMgr as Transaction Manager
    participant BF as Blockifier

    B->>L1P: start_block(SessionState::Propose, height)
    L1P->>TxMgr: start_block()
    Note over TxMgr: Reset staging epoch
    L1P-->>B: Ok

    B->>TxProv: new(max_l1_handler_txs_per_block)
    Note over TxProv: phase = L1

    rect rgb(240, 248, 255)
        Note over TxProv,BF: L1 Handler Phase
        TxProv->>L1P: get_txs(n_txs, height)
        L1P->>TxMgr: get_txs(n_txs, unix_now)

        Note over TxMgr: Filter by:<br/>- Proposable state (Pending)<br/>- Cooldown time passed<br/>- Not already staged

        TxMgr->>TxMgr: mark_staged(tx_hash)
        TxMgr-->>L1P: L1 handler transactions
        L1P-->>TxProv: L1 handler transactions

        TxProv->>TxProv: Convert to InternalConsensusTransaction::L1Handler
        TxProv->>BF: add_txs_to_block(l1_handler_txs)
    end

    Note over TxProv: Switch to Mempool phase when:<br/>- max_l1_handler_txs reached<br/>- no more L1 txs available

    TxProv->>TxProv: phase = Mempool
    Note over TxProv,BF: Continue with mempool transactions...
```

## Block Validation - Validating L1 Transactions

```mermaid
sequenceDiagram
    participant B as Batcher
    participant TxProv as ValidateTransactionProvider
    participant L1P as L1 Provider
    participant TxMgr as Transaction Manager
    participant BF as Blockifier

    B->>L1P: start_block(SessionState::Validate, height)
    L1P->>TxMgr: start_block()
    Note over TxMgr: Reset staging epoch
    L1P-->>B: Ok

    B->>TxProv: new(tx_receiver, l1_provider_client, height)

    rect rgb(255, 245, 238)
        Note over TxProv,BF: Validate received L1 handler transactions
        loop For each L1Handler tx from consensus
            TxProv->>TxProv: recv() from tx_receiver

            TxProv->>L1P: validate(tx_hash, height)
            L1P->>TxMgr: validate_tx(tx_hash, unix_now)

            Note over TxMgr: Check state transitions<br/>(cancellation/consumption timelocks)

            TxMgr-->>L1P: ValidationStatus
            L1P-->>TxProv: ValidationStatus

            opt Validated
                TxMgr->>TxMgr: mark_staged(tx_hash)
                TxProv->>BF: add_txs_to_block([tx])
            end

            opt Invalid (AlreadyIncludedOnL2, CancelledOnL2, ConsumedOnL1, NotFound, AlreadyIncludedInProposedBlock, L1ProviderError)
                Note over TxProv: Fail block validation
            end
        end
    end
```

## Block Commit - Finalizing L1 Transactions

```mermaid
sequenceDiagram
    participant B as Batcher
    participant L1P as L1 Provider
    participant TxMgr as Transaction Manager
    participant Storage as Storage

    Note over B: After block execution completed

    B->>B: Collect consumed_l1_handler_tx_hashes
    B->>B: Collect rejected_tx_hashes
    B->>B: Filter rejected_l1_handler_tx_hashes

    B->>Storage: commit_proposal(height, state_diff)
    Storage-->>B: Ok

    rect rgb(240, 255, 240)
        Note over B,TxMgr: Notify L1 Provider
        B->>L1P: commit_block(consumed_txs, rejected_txs, height)

        L1P->>L1P: apply_commit_block(consumed, rejected)
        L1P->>TxMgr: commit_txs(committed_txs, rejected_txs)

        Note over TxMgr: For committed txs: Pending to Committed
        Note over TxMgr: For rejected txs: Unstage, keep as Pending

        TxMgr->>TxMgr: rollback_staging()
        Note over TxMgr: Increments staging epoch<br/>(unstages all txs for next block)
        L1P->>L1P: increment current_height
        L1P-->>B: Ok
    end

    B->>B: Continue with mempool.commit_block()
```
