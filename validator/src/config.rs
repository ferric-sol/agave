use {
    crate::cli::DefaultArgs,
    clap::ArgMatches,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, path::PathBuf},
    solana_runtime::snapshot_utils::SnapshotVersion,
};

/// TOML-based configuration for the Solana validator
/// All fields are optional to allow partial configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidatorConfig {
    // Network configuration
    pub bind_address: Option<String>,
    pub entrypoint: Option<Vec<String>>,
    pub gossip_port: Option<u16>,
    pub gossip_host: Option<String>,
    pub dynamic_port_range: Option<String>,
    pub allow_private_addr: Option<bool>,
    
    // Ledger configuration
    pub ledger_path: Option<PathBuf>,
    pub accounts_path: Option<Vec<PathBuf>>,
    pub account_snapshot_paths: Option<Vec<PathBuf>>,
    pub limit_ledger_size: Option<u64>,
    
    // RPC configuration
    pub rpc_port: Option<u16>,
    pub rpc_bind_address: Option<String>,
    pub enable_rpc_transaction_history: Option<bool>,
    pub enable_extended_tx_metadata_storage: Option<bool>,
    pub rpc_threads: Option<usize>,
    pub rpc_blocking_threads: Option<usize>,
    pub rpc_max_request_body_size: Option<usize>,
    pub rpc_pubsub_max_active_subscriptions: Option<usize>,
    pub rpc_pubsub_queue_capacity_items: Option<usize>,
    pub rpc_pubsub_queue_capacity_bytes: Option<usize>,
    
    // Performance configuration
    pub accounts_shrink_ratio: Option<f64>,
    pub accounts_shrink_optimize_total_space: Option<bool>,
    pub banking_trace_dir_byte_limit: Option<u64>,
    pub tpu_connection_pool_size: Option<usize>,
    pub tpu_max_connections_per_peer: Option<usize>,
    pub tpu_max_connections_per_ipaddr_per_minute: Option<u64>,
    pub tpu_max_staked_connections: Option<usize>,
    pub tpu_max_unstaked_connections: Option<usize>,
    pub tpu_max_streams_per_ms: Option<usize>,
    
    // Snapshot configuration
    pub snapshot_version: Option<String>,
    pub snapshot_archive_format: Option<String>,
    pub full_snapshot_archive_interval_slots: Option<u64>,
    pub incremental_snapshot_archive_interval_slots: Option<u64>,
    pub maximum_full_snapshot_archives_to_retain: Option<usize>,
    pub maximum_incremental_snapshot_archives_to_retain: Option<usize>,
    pub min_snapshot_download_speed: Option<u64>,
    pub max_snapshot_download_abort: Option<u32>,
    
    // Thread configuration
    pub replay_forks_threads: Option<usize>,
    pub replay_transactions_threads: Option<usize>,
    pub tvu_shred_sigverify_threads: Option<usize>,
    
    // Validator behavior
    pub voting_disabled: Option<bool>,
    pub dev_halt_at_slot: Option<u64>,
    pub wait_for_supermajority: Option<u64>,
    pub expected_genesis_hash: Option<String>,
    pub expected_bank_hash: Option<String>,
    pub expected_shred_version: Option<u16>,
    pub no_voting: Option<bool>,
    pub no_check_vote_account: Option<bool>,
    
    // Identity and security
    pub identity: Option<PathBuf>,
    pub vote_account: Option<String>,
    pub authorized_voter_keypairs: Option<Vec<PathBuf>>,
    pub known_validators: Option<Vec<String>>,
    pub only_known_rpc: Option<bool>,
    
    // Feature flags and development
    pub log_messages_bytes_limit: Option<usize>,
    pub skip_startup_ledger_verification: Option<bool>,
    pub skip_poh_verify: Option<bool>, // Deprecated but kept for compatibility
    pub debug_keys: Option<Vec<String>>,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            bind_address: None,
            entrypoint: None,
            gossip_port: None,
            gossip_host: None,
            dynamic_port_range: None,
            allow_private_addr: None,
            ledger_path: None,
            accounts_path: None,
            account_snapshot_paths: None,
            limit_ledger_size: None,
            rpc_port: None,
            rpc_bind_address: None,
            enable_rpc_transaction_history: None,
            enable_extended_tx_metadata_storage: None,
            rpc_threads: None,
            rpc_blocking_threads: None,
            rpc_max_request_body_size: None,
            rpc_pubsub_max_active_subscriptions: None,
            rpc_pubsub_queue_capacity_items: None,
            rpc_pubsub_queue_capacity_bytes: None,
            accounts_shrink_ratio: None,
            accounts_shrink_optimize_total_space: None,
            banking_trace_dir_byte_limit: None,
            tpu_connection_pool_size: None,
            tpu_max_connections_per_peer: None,
            tpu_max_connections_per_ipaddr_per_minute: None,
            tpu_max_staked_connections: None,
            tpu_max_unstaked_connections: None,
            tpu_max_streams_per_ms: None,
            snapshot_version: None,
            snapshot_archive_format: None,
            full_snapshot_archive_interval_slots: None,
            incremental_snapshot_archive_interval_slots: None,
            maximum_full_snapshot_archives_to_retain: None,
            maximum_incremental_snapshot_archives_to_retain: None,
            min_snapshot_download_speed: None,
            max_snapshot_download_abort: None,
            replay_forks_threads: None,
            replay_transactions_threads: None,
            tvu_shred_sigverify_threads: None,
            voting_disabled: None,
            dev_halt_at_slot: None,
            wait_for_supermajority: None,
            expected_genesis_hash: None,
            expected_bank_hash: None,
            expected_shred_version: None,
            no_voting: None,
            no_check_vote_account: None,
            identity: None,
            vote_account: None,
            authorized_voter_keypairs: None,
            known_validators: None,
            only_known_rpc: None,
            log_messages_bytes_limit: None,
            skip_startup_ledger_verification: None,
            skip_poh_verify: None,
            debug_keys: None,
        }
    }
}

impl ValidatorConfig {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<std::path::Path>>(config_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let config_str = std::fs::read_to_string(config_path.as_ref())
            .map_err(|e| format!("Failed to read config file '{}': {}", config_path.as_ref().display(), e))?;
        
        let config: ValidatorConfig = toml::from_str(&config_str)
            .map_err(|e| format!("Failed to parse TOML config: {}", e))?;
        
        Ok(config)
    }

    /// Save configuration to a TOML file
    pub fn save<P: AsRef<std::path::Path>>(&self, config_path: P) -> Result<(), Box<dyn std::error::Error>> {
        let config_str = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        
        std::fs::write(config_path.as_ref(), config_str)
            .map_err(|e| format!("Failed to write config file '{}': {}", config_path.as_ref().display(), e))?;
        
        Ok(())
    }

    /// Create a default configuration file with comments
    pub fn create_default_config<P: AsRef<std::path::Path>>(config_path: P) -> Result<(), Box<dyn std::error::Error>> {
        let default_toml = r#"# Solana Validator Configuration File
# CLI arguments take precedence over these settings
# All fields are optional - remove or comment out fields to use CLI defaults

# Network Configuration
# bind_address = "127.0.0.1"
# entrypoint = [
#     "entrypoint.mainnet-beta.solana.com:8001",
#     "entrypoint2.mainnet-beta.solana.com:8001"
# ]
# gossip_port = 8001
# gossip_host = "127.0.0.1"
# dynamic_port_range = "8002-10000"
# allow_private_addr = false

# Ledger Configuration
# ledger_path = "./ledger"
# accounts_path = ["./accounts"]
# account_snapshot_paths = ["./snapshots"]
# limit_ledger_size = 50000000

# RPC Configuration  
# rpc_port = 8899
# rpc_bind_address = "127.0.0.1"
# enable_rpc_transaction_history = false
# enable_extended_tx_metadata_storage = false
# rpc_threads = 4
# rpc_blocking_threads = 1
# rpc_max_request_body_size = 52428800
# rpc_pubsub_max_active_subscriptions = 5000
# rpc_pubsub_queue_capacity_items = 100000
# rpc_pubsub_queue_capacity_bytes = 100000000

# Performance Configuration
# accounts_shrink_ratio = 0.8
# accounts_shrink_optimize_total_space = true
# banking_trace_dir_byte_limit = 1000000000
# tpu_connection_pool_size = 4
# tpu_max_connections_per_peer = 8
# tpu_max_connections_per_ipaddr_per_minute = 10
# tpu_max_staked_connections = 2000
# tpu_max_unstaked_connections = 2000
# tpu_max_streams_per_ms = 1000

# Snapshot Configuration
# snapshot_version = "V1_7_0"
# snapshot_archive_format = "zstd"
# full_snapshot_archive_interval_slots = 25000
# incremental_snapshot_archive_interval_slots = 100
# maximum_full_snapshot_archives_to_retain = 2
# maximum_incremental_snapshot_archives_to_retain = 4
# min_snapshot_download_speed = 10485760
# max_snapshot_download_abort = 5

# Thread Configuration
# replay_forks_threads = 1
# replay_transactions_threads = 1
# tvu_shred_sigverify_threads = 1

# Validator Behavior
# voting_disabled = false
# dev_halt_at_slot = 0
# wait_for_supermajority = 0
# expected_genesis_hash = ""
# expected_bank_hash = ""
# expected_shred_version = 0
# no_voting = false

# Identity and Security
# identity = "./validator-keypair.json"
# vote_account = "./vote-account-keypair.json"
# authorized_voter_keypairs = ["./authorized-voter-keypair.json"]
# known_validators = []
# only_known_rpc = false

# Development and Debugging
# log_messages_bytes_limit = 10000
# skip_startup_ledger_verification = false
# debug_keys = []
"#;
        
        std::fs::write(config_path.as_ref(), default_toml)
            .map_err(|e| format!("Failed to write default config file '{}': {}", config_path.as_ref().display(), e))?;
        
        Ok(())
    }
}

/// Merged configuration that combines TOML config with CLI arguments and defaults
/// CLI arguments take precedence over TOML config, which takes precedence over defaults
pub struct MergedConfig {
    pub validator_config: ValidatorConfig,
    pub default_args: DefaultArgs,
}

impl MergedConfig {
    pub fn new(
        matches: &ArgMatches,
        default_args: &DefaultArgs,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Load TOML config if specified
        let mut validator_config = if let Some(config_file) = matches.value_of("config_file") {
            ValidatorConfig::load(config_file)?
        } else {
            ValidatorConfig::default()
        };

        // Override TOML config with CLI arguments where provided
        Self::merge_cli_args(&mut validator_config, matches);

        Ok(Self {
            validator_config,
            default_args: default_args.clone(),
        })
    }

    /// Merge CLI arguments into the validator config, CLI takes precedence
    fn merge_cli_args(config: &mut ValidatorConfig, matches: &ArgMatches) {
        // Network configuration
        if matches.is_present("bind_address") {
            config.bind_address = matches.value_of("bind_address").map(|s| s.to_string());
        }
        if matches.is_present("entrypoint") {
            config.entrypoint = Some(matches.values_of("entrypoint")
                .unwrap()
                .map(|s| s.to_string())
                .collect());
        }
        if matches.is_present("gossip_port") {
            config.gossip_port = matches.value_of("gossip_port").and_then(|s| s.parse().ok());
        }
        if matches.is_present("gossip_host") {
            config.gossip_host = matches.value_of("gossip_host").map(|s| s.to_string());
        }
        if matches.is_present("dynamic_port_range") {
            config.dynamic_port_range = matches.value_of("dynamic_port_range").map(|s| s.to_string());
        }
        if matches.is_present("allow_private_addr") {
            config.allow_private_addr = Some(matches.is_present("allow_private_addr"));
        }
        
        // Ledger configuration
        if matches.is_present("ledger_path") {
            config.ledger_path = matches.value_of("ledger_path").map(PathBuf::from);
        }
        if matches.is_present("account_paths") {
            config.accounts_path = Some(matches.values_of("account_paths")
                .unwrap()
                .map(PathBuf::from)
                .collect());
        }
        if matches.is_present("limit_ledger_size") {
            config.limit_ledger_size = matches.value_of("limit_ledger_size").and_then(|s| s.parse().ok());
        }
        
        // RPC configuration
        if matches.is_present("rpc_port") {
            config.rpc_port = matches.value_of("rpc_port").and_then(|s| s.parse().ok());
        }
        if matches.is_present("rpc_bind_address") {
            config.rpc_bind_address = matches.value_of("rpc_bind_address").map(|s| s.to_string());
        }
        if matches.is_present("enable_rpc_transaction_history") {
            config.enable_rpc_transaction_history = Some(matches.is_present("enable_rpc_transaction_history"));
        }
        if matches.is_present("enable_extended_tx_metadata_storage") {
            config.enable_extended_tx_metadata_storage = Some(matches.is_present("enable_extended_tx_metadata_storage"));
        }
        
        // Performance configuration
        if matches.is_present("accounts_shrink_ratio") {
            config.accounts_shrink_ratio = matches.value_of("accounts_shrink_ratio").and_then(|s| s.parse().ok());
        }
        if matches.is_present("banking_trace_dir_byte_limit") {
            config.banking_trace_dir_byte_limit = matches.value_of("banking_trace_dir_byte_limit").and_then(|s| s.parse().ok());
        }
        
        // Thread configuration
        if matches.is_present("replay_forks_threads") {
            config.replay_forks_threads = matches.value_of("replay_forks_threads").and_then(|s| s.parse().ok());
        }
        
        // Validator behavior
        if matches.is_present("voting_disabled") {
            config.voting_disabled = Some(matches.is_present("voting_disabled"));
        }
        if matches.is_present("dev_halt_at_slot") {
            config.dev_halt_at_slot = matches.value_of("dev_halt_at_slot").and_then(|s| s.parse().ok());
        }
        if matches.is_present("wait_for_supermajority") {
            config.wait_for_supermajority = matches.value_of("wait_for_supermajority").and_then(|s| s.parse().ok());
        }
        if matches.is_present("expected_genesis_hash") {
            config.expected_genesis_hash = matches.value_of("expected_genesis_hash").map(|s| s.to_string());
        }
        if matches.is_present("expected_bank_hash") {
            config.expected_bank_hash = matches.value_of("expected_bank_hash").map(|s| s.to_string());
        }
        if matches.is_present("expected_shred_version") {
            config.expected_shred_version = matches.value_of("expected_shred_version").and_then(|s| s.parse().ok());
        }
        if matches.is_present("no_voting") {
            config.no_voting = Some(matches.is_present("no_voting"));
        }
        
        // Identity
        if matches.is_present("identity") {
            config.identity = matches.value_of("identity").map(PathBuf::from);
        }
        if matches.is_present("vote_account") {
            config.vote_account = matches.value_of("vote_account").map(|s| s.to_string());
        }
        if matches.is_present("authorized_voter_keypairs") {
            config.authorized_voter_keypairs = Some(matches.values_of("authorized_voter_keypairs")
                .unwrap()
                .map(PathBuf::from)
                .collect());
        }
        
        // Development
        if matches.is_present("log_messages_bytes_limit") {
            config.log_messages_bytes_limit = matches.value_of("log_messages_bytes_limit").and_then(|s| s.parse().ok());
        }
        if matches.is_present("skip_startup_ledger_verification") {
            config.skip_startup_ledger_verification = Some(matches.is_present("skip_startup_ledger_verification"));
        }
    }

    /// Get a configuration value, checking TOML config first, then defaults
    pub fn get_bind_address(&self) -> String {
        self.validator_config.bind_address
            .clone()
            .unwrap_or_else(|| self.default_args.bind_address.clone())
    }

    pub fn get_ledger_path(&self) -> PathBuf {
        self.validator_config.ledger_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&self.default_args.ledger_path))
    }

    pub fn get_rpc_threads(&self) -> usize {
        self.validator_config.rpc_threads
            .unwrap_or_else(|| self.default_args.rpc_threads.parse().unwrap_or(4))
    }

    pub fn get_banking_trace_dir_byte_limit(&self) -> u64 {
        self.validator_config.banking_trace_dir_byte_limit
            .unwrap_or_else(|| self.default_args.banking_trace_dir_byte_limit.parse().unwrap_or(1000000000))
    }

    // Add more getter methods as needed for other configuration values
}

/// Generate a sample configuration file with current CLI arguments
pub fn generate_config_from_args(
    matches: &ArgMatches,
    output_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = ValidatorConfig::default();
    MergedConfig::merge_cli_args(&mut config, matches);
    config.save(output_path)?;
    println!("Configuration file generated at: {}", output_path.display());
    Ok(())
} 