# Codebase Reduction Roadmap: Fast Path, Consensus, and Governance Only

This document outlines a strategy to reduce the Sui codebase to only support:
1. **Fast Path Transactions** - Owned-object transactions that execute without full consensus
2. **Consensus** - The consensus mechanism (Mysticeti) for ordering transactions
3. **Governance** - Validator governance, staking, and epoch management

**Important:** This reduction will preserve:
- **Benchmarks** - All benchmarking tools and utilities
- **Tests** - All test suites and testing infrastructure
- **RPC** - All RPC services (JSON-RPC, GraphQL-RPC, KV store)
- **CLI** - All CLI tools

## Overview

The Sui blockchain currently supports multiple transaction execution paths:
- **Fast Path**: Owned-object transactions that can execute immediately after certificate formation
- **Consensus Path**: Shared-object transactions that require consensus ordering before execution
- **Governance**: Validator set management, staking, epoch transitions

## Approach: Removing vs. Starting Fresh

### Recommendation: **Remove from Existing Codebase** ✅

**Why removing is easier:**

1. **Preserves Infrastructure**: Tests, benchmarks, RPC, and CLI all have extensive dependencies on core components. Starting fresh would require rebuilding all of these.

2. **Incremental Safety**: Can remove code in phases, with tests catching breakages at each step:
   - Phase 1: Disable shared object features in protocol config
   - Phase 2: Add rejection logic for shared object transactions
   - Phase 3: Remove shared object code paths
   - Phase 4: Remove indexer/bridge crates

3. **Existing Test Coverage**: ~200+ workspace members have tests that will guide what breaks. Starting fresh means no safety net.

4. **Dependency Management**: The codebase has complex interdependencies (sui-core → consensus → sui-types → execution layers). Removing lets you trace dependencies incrementally rather than rebuilding the entire dependency graph.

5. **Time Investment**: 
   - **Removing**: ~2-4 weeks of focused work
   - **Starting fresh**: ~3-6 months (essentially a rewrite)

6. **Git History**: Preserving history helps understand decisions and allows easier rollback.

**When starting fresh might make sense:**
- If 80%+ of code needs removal
- If architecture needs fundamental restructuring
- If you want a completely clean slate for new contributors

**Given this codebase:**
- ~30-40% removal (shared objects, indexers, bridge, replay)
- Core architecture stays the same (fast path + consensus)
- Need to preserve tests/benchmarks/RPC/CLI (they depend on core)

**→ Removing is the clear winner.**

### Hybrid Approach (Recommended)

1. **Create a feature flag branch** (`reduction-fastpath-only`)
2. **Add protocol config gates** to disable shared object support
3. **Remove incrementally** following the phases below
4. **Keep tests passing** at each step
5. **Once stable**, consider a clean branch merge for final cleanup

## Where to Start

### Phase 1: Analysis and Planning (START HERE)

#### 1.1 Identify Core Components to Keep

**Fast Path Components:**
- `crates/sui-core/src/authority.rs` - Fast path transaction handling (lines ~1169-1178)
- `crates/sui-core/src/execution_scheduler/` - Execution scheduling for fast path
- `crates/sui-core/src/unit_tests/mysticeti_fastpath_execution_tests.rs` - Fast path tests
- Protocol config: `mysticeti_fastpath()` feature flag

**Consensus Components:**
- `consensus/` directory - Full consensus implementation (Mysticeti)
- `crates/sui-core/src/consensus_handler.rs` - Consensus transaction handling
- `crates/sui-core/src/consensus_adapter.rs` - Adapter between authority and consensus
- `crates/sui-core/src/consensus_manager/` - Consensus manager

**Governance Components:**
- `crates/sui-types/src/governance.rs` - Governance types and constants
- `crates/sui-json-rpc/src/governance_api.rs` - Governance API endpoints
- `crates/sui-json-rpc-api/src/governance.rs` - Governance API definitions
- `crates/sui-types/src/sui_system_state/` - System state for validators and staking
- Epoch management in `crates/sui-core/src/authority/authority_per_epoch_store.rs`

#### 1.2 Identify Components to Remove or Reduce

**Shared Object Support (REMOVE):**
- `crates/sui-core/src/authority/shared_object_version_manager.rs`
- `crates/sui-core/src/authority/shared_object_congestion_tracker.rs`
- Shared object handling in `consensus_handler.rs` (lines ~1104-1273)
- `crates/sui-core/src/post_consensus_tx_reorder.rs` - Reordering for shared objects
- Shared object tests in `crates/sui-core/src/unit_tests/`

**Indexers (REMOVE or REDUCE):**
- `crates/sui-indexer/` - Full indexer
- `crates/sui-indexer-alt/` - Alternative indexer
- `crates/sui-indexer-alt-jsonrpc/` - Indexer JSON-RPC
- `crates/sui-indexer-alt-graphql/` - Indexer GraphQL
- `crates/sui-analytics-indexer/` - Analytics indexer
- `crates/sui-checkpoint-blob-indexer/` - Checkpoint blob indexer
- `crates/sui-deepbook-indexer/` - DeepBook indexer
- `crates/sui-bridge-indexer/` - Bridge indexer

**RPC Services (KEEP):**
- `crates/sui-graphql-rpc/` - GraphQL RPC (keep)
- `crates/sui-json-rpc/` - JSON-RPC (keep)
- `crates/sui-kvstore/` - Key-value store RPC (keep)

**Tools and Utilities (KEEP):**
- `crates/sui-benchmark/` - Benchmarking tools (keep)
- `crates/sui-cluster-test/` - Cluster testing (keep)
- `crates/sui-single-node-benchmark/` - Single node benchmarks (keep)
- `crates/sui-rpc-benchmark/` - RPC benchmarks (keep)
- `crates/sui-rpc-loadgen/` - RPC load generation (keep)

**Tools and Utilities (REMOVE):**
- `crates/sui-replay/` - Transaction replay
- `crates/sui-replay-2/` - Transaction replay v2
- `crates/sui-surfer/` - Surfer tool

**Bridge (REMOVE):**
- `bridge/` directory - Bridge functionality

**DApps and Examples (REMOVE):**
- `dapps/` directory - Example dapps
- `examples/` directory - Example code
- `sdk/` - Keep only minimal SDK if needed for governance

**Other Services (KEEP):**
- `crates/sui-faucet/` - Faucet service (keep for testing)
- `crates/sui-test-validator/` - Test validator (keep for testing)
- `crates/sui-tool/` - CLI tools (keep all)

### Phase 2: Code Changes

#### 2.1 Remove Shared Object Support

**Key Files to Modify:**

1. **`crates/sui-core/src/authority.rs`**
   - Remove shared object transaction handling
   - Keep only fast path transaction logic
   - Remove `is_consensus_tx()` checks for shared objects

2. **`crates/sui-core/src/consensus_handler.rs`**
   - Remove shared object version assignment (lines ~1264-1273)
   - Remove shared object congestion tracking
   - Keep only consensus commit prologue and governance transactions
   - Remove `process_consensus_transaction_shared_object_versions()`

3. **`crates/sui-core/src/authority_server.rs`**
   - Remove shared object transaction paths
   - Simplify `handle_submit_to_consensus()` to only handle governance

4. **`crates/sui-types/src/transaction.rs`**
   - Remove or simplify `is_consensus_tx()` to only check for governance transactions
   - Remove shared object transaction types

5. **`crates/sui-core/src/transaction_orchestrator.rs`**
   - Remove shared object transaction handling
   - Simplify to only handle fast path transactions

#### 2.2 Simplify Consensus Handler

**File: `crates/sui-core/src/consensus_handler.rs`**

Remove:
- Shared object version management
- Shared object congestion tracking
- Post-consensus transaction reordering for shared objects
- Deferred transaction handling for shared objects

Keep:
- Consensus commit prologue
- Governance transactions (epoch changes, validator set updates)
- Fast path transaction certification from consensus blocks

#### 2.3 Update Protocol Config

**File: `crates/sui-protocol-config/src/lib.rs`**

- Ensure `mysticeti_fastpath()` is enabled
- Remove or disable shared object related features
- Update feature flags to reflect reduced functionality

### Phase 3: Testing and Validation

#### 3.1 Update Tests

**Keep:**
- `crates/sui-core/src/unit_tests/mysticeti_fastpath_execution_tests.rs`
- `crates/sui-core/src/unit_tests/consensus_tests.rs` (governance-related)
- Governance-related tests
- All other test files (tests are kept)

**Remove or Update:**
- Only shared object tests need to be removed or updated to verify rejection
- Indexer tests (only if indexers are removed)
- Bridge tests (only if bridge is removed)

#### 3.2 Integration Testing

- Test fast path transaction flow end-to-end
- Test consensus for governance transactions
- Test epoch transitions and validator set changes
- Verify no shared object transactions are accepted

### Phase 4: Build System and Dependencies

#### 4.1 Update Cargo Workspace

**File: `Cargo.toml` (workspace root)**

Remove workspace members for:
- All indexer crates
- Bridge crates
- Replay crates
- Surfer tool

Keep workspace members for:
- All benchmark crates (sui-benchmark, sui-cluster-test, sui-single-node-benchmark, sui-rpc-benchmark, sui-rpc-loadgen)
- All CLI tools (sui-tool)
- All RPC services (sui-json-rpc, sui-graphql-rpc, sui-kvstore)
- All test utilities (sui-faucet, sui-test-validator)

#### 4.2 Update CI/CD

**Files: `.github/workflows/*.yml`**

- Remove indexer build jobs
- Remove bridge tests
- Remove unnecessary test suites
- Keep only fast path, consensus, and governance tests

## Implementation Order

### Step 1: Start with Protocol Config (EASIEST)
1. Review `crates/sui-protocol-config/src/lib.rs`
2. Identify and disable shared object features
3. Ensure fast path is enabled

### Step 2: Remove Shared Object Code (MEDIUM)
1. Start with `crates/sui-core/src/authority/shared_object_*` files
2. Remove shared object handling from `consensus_handler.rs`
3. Update `authority.rs` to reject shared object transactions

### Step 3: Simplify Consensus Handler (MEDIUM)
1. Remove shared object version assignment
2. Remove congestion tracking
3. Keep only governance transaction handling

### Step 4: Remove Indexers and Replay Tools (EASY)
1. Remove indexer crates from workspace
2. Remove replay and surfer tool crates (keep benchmarks and CLI tools)
3. Update build scripts

### Step 5: Update Tests (MEDIUM)
1. Update shared object tests to verify they are rejected
2. Keep all other tests
3. Add tests to verify shared objects are rejected

### Step 6: Clean Up Dependencies (EASY)
1. Remove unused dependencies
2. Update Cargo.toml files
3. Run `cargo check` to verify

## Key Files to Focus On

### Critical Files (Must Modify):
1. `crates/sui-core/src/authority.rs` - Main authority logic
2. `crates/sui-core/src/consensus_handler.rs` - Consensus transaction handling
3. `crates/sui-core/src/authority_server.rs` - RPC handlers
4. `crates/sui-types/src/transaction.rs` - Transaction types
5. `crates/sui-protocol-config/src/lib.rs` - Protocol configuration

### Important Files (Should Review):
1. `crates/sui-core/src/transaction_orchestrator.rs` - Transaction orchestration
2. `crates/sui-core/src/consensus_adapter.rs` - Consensus adapter
3. `crates/sui-core/src/quorum_driver/` - Quorum driver
4. `crates/sui-core/src/execution_scheduler/` - Execution scheduler

### Files to Remove:
1. All files in `crates/sui-core/src/authority/shared_object_*`
2. `crates/sui-core/src/post_consensus_tx_reorder.rs`
3. All indexer crates
4. All replay crates
5. Bridge directory

## Validation Checklist

After reduction, verify:
- [ ] Fast path transactions execute correctly
- [ ] Consensus handles governance transactions
- [ ] Epoch transitions work
- [ ] Validator set changes work
- [ ] Staking/unstaking works
- [ ] Shared object transactions are rejected
- [ ] All tests pass
- [ ] Code compiles without warnings
- [ ] No dead code remains

## Notes

- **Governance transactions** still need consensus, so consensus is not fully removed
- **Fast path** transactions bypass consensus but still need certificate formation
- **Epoch management** requires consensus for validator set changes
- Consider keeping minimal checkpointing for state synchronization
- Consider keeping minimal RPC for governance queries

## Getting Help

- Review `CLAUDE.md` for development guidelines
- Check crate-specific `CLAUDE.md` files
- Look at `crates/sui-core/src/unit_tests/mysticeti_fastpath_execution_tests.rs` for fast path examples
- Review `crates/sui-types/src/governance.rs` for governance structures
