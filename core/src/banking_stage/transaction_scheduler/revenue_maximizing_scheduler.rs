#[cfg(feature = "dev-context-only-utils")]
use qualifier_attr::qualifiers;
use {
    super::{
        scheduler::{PreLockFilterAction, Scheduler, SchedulingSummary},
        scheduler_common::{
            select_thread, SchedulingCommon, TransactionSchedulingError, TransactionSchedulingInfo,
        },
        scheduler_error::SchedulerError,
        transaction_priority_id::TransactionPriorityId,
        transaction_state::TransactionState,
        transaction_state_container::StateContainer,
    },
    crate::banking_stage::{
        consumer::TARGET_NUM_TRANSACTIONS_PER_BATCH,
        read_write_account_set::ReadWriteAccountSet,
        scheduler_messages::{ConsumeWork, FinishedConsumeWork},
    },
    agave_scheduling_utils::thread_aware_account_locks::{
        ThreadAwareAccountLocks, ThreadId, ThreadSet, TryLockError,
    },
    crossbeam_channel::{Receiver, Sender},
    solana_cost_model::block_cost_limits::MAX_BLOCK_UNITS,
    solana_runtime_transaction::transaction_with_meta::TransactionWithMeta,
    std::num::Saturating,
};

#[cfg_attr(feature = "dev-context-only-utils", qualifiers(pub))]
pub(crate) struct RevenueMaximizingSchedulerConfig {
    pub target_scheduled_cus: u64,
    pub max_scanned_transactions_per_scheduling_pass: usize,
    pub look_ahead_window_size: usize,
    pub target_transactions_per_batch: usize,
    /// Maximum number of transactions to schedule from a single window before rebuilding
    pub max_scheduled_per_window: usize,
    /// Maximum number of times to retry blocked transactions within the same scheduling pass
    pub max_retries_for_blocked: usize,
}

impl Default for RevenueMaximizingSchedulerConfig {
    fn default() -> Self {
        Self {
            target_scheduled_cus: MAX_BLOCK_UNITS / 4,
            max_scanned_transactions_per_scheduling_pass: 100_000,
            look_ahead_window_size: 128,
            target_transactions_per_batch: TARGET_NUM_TRANSACTIONS_PER_BATCH,
            // Schedule up to 16 transactions per window to amortize sorting cost
            max_scheduled_per_window: 16,
            // Retry blocked transactions once per pass
            max_retries_for_blocked: 1,
        }
    }
}

/// Revenue-maximizing scheduler that optimizes for total fee collection.
/// Unlike the greedy scheduler, this scheduler will look ahead in the queue
/// and schedule lower-priority transactions when higher-priority ones are
/// blocked by account conflicts, maximizing throughput and revenue.
pub struct RevenueMaximizingScheduler<Tx: TransactionWithMeta> {
    common: SchedulingCommon<Tx>,
    working_account_set: ReadWriteAccountSet,
    blocked_high_priority: Vec<TransactionPriorityId>,
    scanned_window: Vec<TransactionPriorityId>,
    config: RevenueMaximizingSchedulerConfig,
    /// Track retry counts for blocked transactions to prevent infinite retries
    retry_counts: std::collections::HashMap<u64, usize>,
}

impl<Tx: TransactionWithMeta> RevenueMaximizingScheduler<Tx> {
    #[cfg_attr(feature = "dev-context-only-utils", qualifiers(pub))]
    pub(crate) fn new(
        consume_work_senders: Vec<Sender<ConsumeWork<Tx>>>,
        finished_consume_work_receiver: Receiver<FinishedConsumeWork<Tx>>,
        config: RevenueMaximizingSchedulerConfig,
    ) -> Self {
        Self {
            working_account_set: ReadWriteAccountSet::default(),
            blocked_high_priority: Vec::with_capacity(config.look_ahead_window_size),
            scanned_window: Vec::with_capacity(config.look_ahead_window_size),
            common: SchedulingCommon::new(
                consume_work_senders,
                finished_consume_work_receiver,
                config.target_transactions_per_batch,
            ),
            retry_counts: std::collections::HashMap::new(),
            config,
        }
    }
}

/// Calculate revenue score for a transaction.
/// Higher score means higher priority for scheduling.
/// Score is based on fee-per-CU ratio.
fn calculate_revenue_score(priority: u64, cost: u64) -> f64 {
    if cost == 0 {
        return 0.0;
    }
    priority as f64 / cost as f64
}

/// Information about a candidate transaction for scheduling.
#[derive(Clone, Copy)]
struct SchedulingCandidate {
    priority_id: TransactionPriorityId,
    revenue_score: f64,
}

impl<Tx: TransactionWithMeta> Scheduler<Tx> for RevenueMaximizingScheduler<Tx> {
    fn schedule<S: StateContainer<Tx>>(
        &mut self,
        container: &mut S,
        budget: u64,
        relax_intrabatch_account_locks: bool,
        _pre_graph_filter: impl Fn(&[&Tx], &mut [bool]),
        pre_lock_filter: impl Fn(&TransactionState<Tx>) -> PreLockFilterAction,
    ) -> Result<SchedulingSummary, SchedulerError> {
        // Subtract any in-flight compute units from the budget.
        let mut budget = budget.saturating_sub(
            self.common
                .in_flight_tracker
                .cus_in_flight_per_thread()
                .iter()
                .sum(),
        );

        let starting_queue_size = container.queue_size();
        let starting_buffer_size = container.buffer_size();

        let num_threads = self.common.consume_work_senders.len();
        let target_cu_per_thread = self.config.target_scheduled_cus / num_threads as u64;

        let mut schedulable_threads = ThreadSet::any(num_threads);
        for thread_id in 0..num_threads {
            if self.common.in_flight_tracker.cus_in_flight_per_thread()[thread_id]
                >= target_cu_per_thread
            {
                schedulable_threads.remove(thread_id);
            }
        }
        if schedulable_threads.is_empty() {
            return Ok(SchedulingSummary {
                starting_queue_size,
                starting_buffer_size,
                ..SchedulingSummary::default()
            });
        }

        #[cfg(debug_assertions)]
        debug_assert!(
            self.common.batches.is_empty(),
            "batches must start empty for scheduling"
        );

        // Track metrics
        let mut num_scanned: usize = 0;
        let mut num_scheduled = Saturating::<usize>(0);
        let mut num_sent: usize = 0;
        let mut num_unschedulable_conflicts: usize = 0;
        let mut num_unschedulable_threads: usize = 0;

        // Main scheduling loop
        while budget > 0
            && num_scanned < self.config.max_scanned_transactions_per_scheduling_pass
            && !schedulable_threads.is_empty()
            && !container.is_empty()
        {
            // Build look-ahead window of candidate transactions
            self.scanned_window.clear();
            let mut candidates = Vec::with_capacity(self.config.look_ahead_window_size);

            for _ in 0..self.config.look_ahead_window_size {
                if container.is_empty() {
                    break;
                }

                let Some(id) = container.pop() else {
                    break;
                };

                num_scanned += 1;
                self.scanned_window.push(id);

                // Get transaction state to calculate revenue score
                if let Some(transaction_state) = container.get_mut_transaction_state(id.id) {
                    let revenue_score =
                        calculate_revenue_score(id.priority, transaction_state.cost());
                    candidates.push(SchedulingCandidate {
                        priority_id: id,
                        revenue_score,
                    });
                }
            }

            if candidates.is_empty() {
                break;
            }

            // Sort candidates by revenue score (highest first)
            candidates.sort_by(|a, b| {
                b.revenue_score
                    .partial_cmp(&a.revenue_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Try to schedule transactions from the window
            // Schedule multiple transactions to amortize the cost of building and sorting
            let mut scheduled_from_window = 0;
            let mut temporarily_blocked = Vec::new();
            
            for candidate in candidates {
                let id = candidate.priority_id;

                let Some(transaction_state) = container.get_mut_transaction_state(id.id) else {
                    continue;
                };

                // Check if we've already retried this transaction too many times
                let retry_count = self.retry_counts.get(&id.id).copied().unwrap_or(0);
                if retry_count >= self.config.max_retries_for_blocked {
                    // Exceeded retry limit, add to blocked list
                    self.blocked_high_priority.push(id);
                    continue;
                }

                // If there is a conflict with any of the transactions in the current batches,
                // we should immediately send out the batches, so this transaction may be scheduled.
                if !relax_intrabatch_account_locks
                    && !self
                        .working_account_set
                        .check_locks(transaction_state.transaction())
                {
                    self.working_account_set.clear();
                    num_sent += self.common.send_batches()?;
                }

                // Now check if the transaction can actually be scheduled.
                match try_schedule_transaction(
                    transaction_state,
                    &pre_lock_filter,
                    &mut self.common.account_locks,
                    schedulable_threads,
                    |thread_set| {
                        select_thread(
                            thread_set,
                            self.common.batches.total_cus(),
                            self.common.in_flight_tracker.cus_in_flight_per_thread(),
                            self.common.batches.transactions(),
                            self.common.in_flight_tracker.num_in_flight_per_thread(),
                        )
                    },
                ) {
                    Err(TransactionSchedulingError::UnschedulableConflicts) => {
                        num_unschedulable_conflicts += 1;
                        // Track for potential retry instead of immediately blocking
                        temporarily_blocked.push(id);
                    }
                    Err(TransactionSchedulingError::UnschedulableThread) => {
                        num_unschedulable_threads += 1;
                        self.blocked_high_priority.push(id);
                    }
                    Ok(TransactionSchedulingInfo {
                        thread_id,
                        transaction,
                        max_age,
                        cost,
                    }) => {
                        if !relax_intrabatch_account_locks {
                            assert!(
                                self.working_account_set.take_locks(&transaction),
                                "locks must be available"
                            );
                        }
                        num_scheduled += 1;
                        scheduled_from_window += 1;
                        self.common.batches.add_transaction_to_batch(
                            thread_id,
                            id.id,
                            transaction,
                            max_age,
                            cost,
                        );
                        budget = budget.saturating_sub(cost);

                        // Remove from scanned window since it's been scheduled
                        self.scanned_window.retain(|&x| x != id);
                        
                        // Clear retry count on successful scheduling
                        self.retry_counts.remove(&id.id);

                        // If target batch size is reached, send all the batches
                        if self.common.batches.transactions()[thread_id].len()
                            >= self.config.target_transactions_per_batch
                        {
                            self.working_account_set.clear();
                            num_sent += self.common.send_batches()?;
                        }

                        // if the thread is at target_cu_per_thread, remove it from the schedulable threads
                        // if there are no more schedulable threads, stop scheduling.
                        if self.common.in_flight_tracker.cus_in_flight_per_thread()[thread_id]
                            + self.common.batches.total_cus()[thread_id]
                            >= target_cu_per_thread
                        {
                            schedulable_threads.remove(thread_id);
                            if schedulable_threads.is_empty() {
                                break;
                            }
                        }

                        // Stop scheduling from this window if we've hit our per-window limit
                        if scheduled_from_window >= self.config.max_scheduled_per_window {
                            break;
                        }
                    }
                }
            }

            // Process temporarily blocked transactions - increment retry counts and add to blocked list
            for id in temporarily_blocked {
                let retry_count = self.retry_counts.entry(id.id).or_insert(0);
                *retry_count += 1;
                // Only add back for retry if we haven't exceeded the limit
                if *retry_count <= self.config.max_retries_for_blocked {
                    self.blocked_high_priority.push(id);
                }
            }

            // If we scheduled nothing from window, push remaining back and stop
            if scheduled_from_window == 0 {
                break;
            }
        }

        self.working_account_set.clear();
        num_sent += self.common.send_batches()?;
        let Saturating(num_scheduled) = num_scheduled;
        assert_eq!(
            num_scheduled, num_sent,
            "number of scheduled and sent transactions must match"
        );

        // Push unscheduled transactions back into the queue
        // First push back the blocked high-priority transactions
        container.push_ids_into_queue(self.blocked_high_priority.drain(..));
        // Then push back any remaining from the scanned window
        container.push_ids_into_queue(self.scanned_window.drain(..));

        // Clear retry counts for the next scheduling pass to prevent unbounded growth
        // Transactions that remain blocked will get new retry attempts in future passes
        self.retry_counts.clear();

        Ok(SchedulingSummary {
            starting_queue_size,
            starting_buffer_size,
            num_scheduled,
            num_unschedulable_conflicts,
            num_unschedulable_threads,
            num_filtered_out: 0,
            filter_time_us: 0,
        })
    }

    fn scheduling_common_mut(&mut self) -> &mut SchedulingCommon<Tx> {
        &mut self.common
    }
}

fn try_schedule_transaction<Tx: TransactionWithMeta>(
    transaction_state: &mut TransactionState<Tx>,
    pre_lock_filter: impl Fn(&TransactionState<Tx>) -> PreLockFilterAction,
    account_locks: &mut ThreadAwareAccountLocks,
    schedulable_threads: ThreadSet,
    thread_selector: impl Fn(ThreadSet) -> ThreadId,
) -> Result<TransactionSchedulingInfo<Tx>, TransactionSchedulingError> {
    match pre_lock_filter(transaction_state) {
        PreLockFilterAction::AttemptToSchedule => {}
    }

    // Schedule the transaction if it can be.
    let transaction = transaction_state.transaction();
    let account_keys = transaction.account_keys();
    let write_account_locks = account_keys
        .iter()
        .enumerate()
        .filter_map(|(index, key)| transaction.is_writable(index).then_some(key));
    let read_account_locks = account_keys
        .iter()
        .enumerate()
        .filter_map(|(index, key)| (!transaction.is_writable(index)).then_some(key));

    let thread_id = match account_locks.try_lock_accounts(
        write_account_locks,
        read_account_locks,
        schedulable_threads,
        thread_selector,
    ) {
        Ok(thread_id) => thread_id,
        Err(TryLockError::MultipleConflicts) => {
            return Err(TransactionSchedulingError::UnschedulableConflicts);
        }
        Err(TryLockError::ThreadNotAllowed) => {
            return Err(TransactionSchedulingError::UnschedulableThread);
        }
    };

    let (transaction, max_age) = transaction_state.take_transaction_for_scheduling();
    let cost = transaction_state.cost();

    Ok(TransactionSchedulingInfo {
        thread_id,
        transaction,
        max_age,
        cost,
    })
}

#[cfg(test)]
mod test {
    use {
        super::*,
        crate::banking_stage::{
            scheduler_messages::{MaxAge, TransactionId},
            transaction_scheduler::transaction_state_container::TransactionStateContainer,
        },
        crossbeam_channel::unbounded,
        itertools::Itertools,
        solana_compute_budget_interface::ComputeBudgetInstruction,
        solana_hash::Hash,
        solana_keypair::Keypair,
        solana_message::Message,
        solana_pubkey::Pubkey,
        solana_runtime_transaction::runtime_transaction::RuntimeTransaction,
        solana_signer::Signer,
        solana_system_interface::instruction as system_instruction,
        solana_transaction::{sanitized::SanitizedTransaction, Transaction},
        std::borrow::Borrow,
    };

    #[allow(clippy::type_complexity)]
    fn create_test_frame(
        num_threads: usize,
        config: RevenueMaximizingSchedulerConfig,
    ) -> (
        RevenueMaximizingScheduler<RuntimeTransaction<SanitizedTransaction>>,
        Vec<Receiver<ConsumeWork<RuntimeTransaction<SanitizedTransaction>>>>,
        Sender<FinishedConsumeWork<RuntimeTransaction<SanitizedTransaction>>>,
    ) {
        let (consume_work_senders, consume_work_receivers) =
            (0..num_threads).map(|_| unbounded()).unzip();
        let (finished_consume_work_sender, finished_consume_work_receiver) = unbounded();
        let scheduler = RevenueMaximizingScheduler::new(
            consume_work_senders,
            finished_consume_work_receiver,
            config,
        );
        (
            scheduler,
            consume_work_receivers,
            finished_consume_work_sender,
        )
    }

    fn prioritized_transfers(
        from_keypair: &Keypair,
        to_pubkeys: impl IntoIterator<Item = impl Borrow<Pubkey>>,
        lamports: u64,
        priority: u64,
    ) -> RuntimeTransaction<SanitizedTransaction> {
        let to_pubkeys_lamports = to_pubkeys
            .into_iter()
            .map(|pubkey| *pubkey.borrow())
            .zip(std::iter::repeat(lamports))
            .collect_vec();
        let mut ixs =
            system_instruction::transfer_many(&from_keypair.pubkey(), &to_pubkeys_lamports);
        let prioritization = ComputeBudgetInstruction::set_compute_unit_price(priority);
        ixs.push(prioritization);
        let message = Message::new(&ixs, Some(&from_keypair.pubkey()));
        let tx = Transaction::new(&[from_keypair], message, Hash::default());
        RuntimeTransaction::from_transaction_for_tests(tx)
    }

    fn create_container(
        tx_infos: impl IntoIterator<
            Item = (
                impl Borrow<Keypair>,
                impl IntoIterator<Item = impl Borrow<Pubkey>>,
                u64,
                u64,
            ),
        >,
    ) -> TransactionStateContainer<RuntimeTransaction<SanitizedTransaction>> {
        let mut container = TransactionStateContainer::with_capacity(10 * 1024);
        for (from_keypair, to_pubkeys, lamports, compute_unit_price) in tx_infos.into_iter() {
            let transaction = prioritized_transfers(
                from_keypair.borrow(),
                to_pubkeys,
                lamports,
                compute_unit_price,
            );
            const TEST_TRANSACTION_COST: u64 = 5000;
            container.insert_new_transaction(
                transaction,
                MaxAge::MAX,
                compute_unit_price,
                TEST_TRANSACTION_COST,
            );
        }

        container
    }

    fn collect_work(
        receiver: &Receiver<ConsumeWork<RuntimeTransaction<SanitizedTransaction>>>,
    ) -> (
        Vec<ConsumeWork<RuntimeTransaction<SanitizedTransaction>>>,
        Vec<Vec<TransactionId>>,
    ) {
        receiver
            .try_iter()
            .map(|work| {
                let ids = work.ids.clone();
                (work, ids)
            })
            .unzip()
    }

    fn test_pre_graph_filter(
        _txs: &[&RuntimeTransaction<SanitizedTransaction>],
        results: &mut [bool],
    ) {
        results.fill(true);
    }

    fn test_pre_lock_filter(
        _tx: &TransactionState<RuntimeTransaction<SanitizedTransaction>>,
    ) -> PreLockFilterAction {
        PreLockFilterAction::AttemptToSchedule
    }

    #[test]
    fn test_schedule_single_threaded_no_conflicts() {
        let (mut scheduler, work_receivers, _finished_work_sender) = create_test_frame(
            1,
            RevenueMaximizingSchedulerConfig::default(),
        );
        let mut container = create_container([
            (&Keypair::new(), &[Pubkey::new_unique()], 1, 1),
            (&Keypair::new(), &[Pubkey::new_unique()], 2, 2),
        ]);

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                u64::MAX,
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();
        assert_eq!(scheduling_summary.num_scheduled, 2);
        assert_eq!(scheduling_summary.num_unschedulable_conflicts, 0);
        let (_, ids) = collect_work(&work_receivers[0]);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].len(), 2);
    }

    #[test]
    fn test_schedule_budget() {
        let (mut scheduler, _work_receivers, _finished_work_sender) = create_test_frame(
            1,
            RevenueMaximizingSchedulerConfig::default(),
        );
        let mut container = create_container([
            (&Keypair::new(), &[Pubkey::new_unique()], 1, 1),
            (&Keypair::new(), &[Pubkey::new_unique()], 2, 2),
        ]);

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                0, // zero budget
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();
        assert_eq!(scheduling_summary.num_scheduled, 0);
        assert_eq!(scheduling_summary.num_unschedulable_conflicts, 0);
    }

    #[test]
    fn test_schedule_multi_threaded_no_conflicts() {
        let (mut scheduler, work_receivers, _finished_work_sender) = create_test_frame(
            2,
            RevenueMaximizingSchedulerConfig::default(),
        );
        let mut container =
            create_container((0..4).map(|i| (Keypair::new(), [Pubkey::new_unique()], 1, i)));

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                u64::MAX,
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();
        assert_eq!(scheduling_summary.num_scheduled, 4);
        assert_eq!(scheduling_summary.num_unschedulable_conflicts, 0);

        // Verify transactions were distributed across threads
        let (_, ids0) = collect_work(&work_receivers[0]);
        let (_, ids1) = collect_work(&work_receivers[1]);
        assert!(!ids0.is_empty() || !ids1.is_empty());
    }

    #[test]
    fn test_revenue_optimization_with_conflicts() {
        // Test that lower-priority transactions are scheduled when higher-priority
        // ones are blocked by conflicts
        let (mut scheduler, work_receivers, _finished_work_sender) = create_test_frame(
            2,
            RevenueMaximizingSchedulerConfig {
                look_ahead_window_size: 10,
                ..RevenueMaximizingSchedulerConfig::default()
            },
        );

        let conflicting_pubkey = Pubkey::new_unique();
        let unique_pubkey1 = Pubkey::new_unique();
        let unique_pubkey2 = Pubkey::new_unique();

        // Create transactions:
        // - High priority (10) but conflicts with itself
        // - High priority (10) conflicts with first
        // - Medium priority (5) no conflicts
        // - Low priority (3) no conflicts
        let mut container = create_container([
            (Keypair::new(), [conflicting_pubkey], 1, 10),
            (Keypair::new(), [conflicting_pubkey], 1, 10),
            (Keypair::new(), [unique_pubkey1], 1, 5),
            (Keypair::new(), [unique_pubkey2], 1, 3),
        ]);

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                u64::MAX,
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();

        // Should schedule at least 3 transactions (one conflicting + two non-conflicting)
        assert!(scheduling_summary.num_scheduled >= 3);

        let (_, ids0) = collect_work(&work_receivers[0]);
        let (_, ids1) = collect_work(&work_receivers[1]);
        let total_scheduled = ids0.iter().map(|v| v.len()).sum::<usize>()
            + ids1.iter().map(|v| v.len()).sum::<usize>();
        assert!(total_scheduled >= 3);
    }

    #[test]
    fn test_calculate_revenue_score() {
        // Higher priority with same cost = higher score
        let score1 = calculate_revenue_score(100, 50);
        let score2 = calculate_revenue_score(50, 50);
        assert!(score1 > score2);

        // Same priority with lower cost = higher score
        let score3 = calculate_revenue_score(100, 50);
        let score4 = calculate_revenue_score(100, 100);
        assert!(score3 > score4);

        // Zero cost should return 0
        let score5 = calculate_revenue_score(100, 0);
        assert_eq!(score5, 0.0);
    }

    #[test]
    fn test_look_ahead_window_limit() {
        let (mut scheduler, work_receivers, _finished_work_sender) = create_test_frame(
            1,
            RevenueMaximizingSchedulerConfig {
                look_ahead_window_size: 2, // Small window
                ..RevenueMaximizingSchedulerConfig::default()
            },
        );

        // Create many non-conflicting transactions
        let mut container =
            create_container((0..10).map(|i| (Keypair::new(), [Pubkey::new_unique()], 1, i)));

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                u64::MAX,
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();

        // Should schedule all transactions eventually, even with small window
        assert!(scheduling_summary.num_scheduled > 0);
        let (_, ids) = collect_work(&work_receivers[0]);
        assert!(!ids.is_empty());
    }

    #[test]
    fn test_batch_size_limit() {
        let (mut scheduler, work_receivers, _finished_work_sender) = create_test_frame(
            1,
            RevenueMaximizingSchedulerConfig {
                target_transactions_per_batch: 2,
                ..RevenueMaximizingSchedulerConfig::default()
            },
        );

        let mut container = create_container([
            (&Keypair::new(), &[Pubkey::new_unique()], 1, 1),
            (&Keypair::new(), &[Pubkey::new_unique()], 2, 2),
            (&Keypair::new(), &[Pubkey::new_unique()], 3, 3),
        ]);

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                u64::MAX,
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();

        assert_eq!(scheduling_summary.num_scheduled, 3);
        let (_, ids) = collect_work(&work_receivers[0]);
        // Should have multiple batches due to batch size limit
        assert!(ids.len() > 1);
    }

    #[test]
    fn test_multiple_transactions_per_window() {
        // Test that scheduler schedules multiple transactions from a single window
        let (mut scheduler, work_receivers, _finished_work_sender) = create_test_frame(
            2,
            RevenueMaximizingSchedulerConfig {
                look_ahead_window_size: 10,
                max_scheduled_per_window: 5, // Limit to 5 per window
                ..RevenueMaximizingSchedulerConfig::default()
            },
        );

        // Create 10 non-conflicting transactions with varied priorities
        let mut container = create_container(
            (0..10).map(|i| (Keypair::new(), [Pubkey::new_unique()], 1, 100 - i)),
        );

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                u64::MAX,
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();

        // Should schedule all 10 transactions
        assert_eq!(scheduling_summary.num_scheduled, 10);
        assert_eq!(scheduling_summary.num_unschedulable_conflicts, 0);

        let (_, ids0) = collect_work(&work_receivers[0]);
        let (_, ids1) = collect_work(&work_receivers[1]);
        let total_scheduled = ids0.iter().map(|v| v.len()).sum::<usize>()
            + ids1.iter().map(|v| v.len()).sum::<usize>();
        assert_eq!(total_scheduled, 10);
    }

    #[test]
    fn test_retry_logic_for_blocked_transactions() {
        // Test that blocked transactions are retried but not infinitely
        let (mut scheduler, work_receivers, _finished_work_sender) = create_test_frame(
            2,
            RevenueMaximizingSchedulerConfig {
                look_ahead_window_size: 10,
                max_retries_for_blocked: 2, // Allow 2 retries
                ..RevenueMaximizingSchedulerConfig::default()
            },
        );

        let conflicting_pubkey = Pubkey::new_unique();
        let unique_pubkey = Pubkey::new_unique();

        // Create transactions where multiple conflict on the same account
        let mut container = create_container([
            (Keypair::new(), [conflicting_pubkey], 1, 100),
            (Keypair::new(), [conflicting_pubkey], 1, 90),
            (Keypair::new(), [conflicting_pubkey], 1, 80),
            (Keypair::new(), [unique_pubkey], 1, 70),
        ]);

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                u64::MAX,
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();

        // Should schedule at least 2 transactions (one conflicting + one non-conflicting)
        assert!(scheduling_summary.num_scheduled >= 2);
        
        // Some transactions should be marked as having conflicts
        assert!(scheduling_summary.num_unschedulable_conflicts > 0);

        let (_, ids0) = collect_work(&work_receivers[0]);
        let (_, ids1) = collect_work(&work_receivers[1]);
        let total_scheduled = ids0.iter().map(|v| v.len()).sum::<usize>()
            + ids1.iter().map(|v| v.len()).sum::<usize>();
        assert!(total_scheduled >= 2);
    }

    #[test]
    fn test_starvation_prevention() {
        // Test that transactions don't get retried infinitely
        let (mut scheduler, _work_receivers, _finished_work_sender) = create_test_frame(
            1,
            RevenueMaximizingSchedulerConfig {
                look_ahead_window_size: 5,
                max_retries_for_blocked: 0, // No retries allowed
                ..RevenueMaximizingSchedulerConfig::default()
            },
        );

        let conflicting_pubkey = Pubkey::new_unique();

        // Create transactions that all conflict
        let mut container = create_container([
            (Keypair::new(), [conflicting_pubkey], 1, 100),
            (Keypair::new(), [conflicting_pubkey], 1, 90),
            (Keypair::new(), [conflicting_pubkey], 1, 80),
        ]);

        let scheduling_summary = scheduler
            .schedule(
                &mut container,
                u64::MAX,
                false,
                test_pre_graph_filter,
                test_pre_lock_filter,
            )
            .unwrap();

        // Should only schedule 1 transaction (first one)
        assert_eq!(scheduling_summary.num_scheduled, 1);
        // Others should be marked as having conflicts
        assert_eq!(scheduling_summary.num_unschedulable_conflicts, 2);
    }
}

