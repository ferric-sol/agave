# Solana Validator TOML Configuration

The Solana validator now supports TOML-based configuration files in addition to command-line arguments. This feature provides better organization and management of validator settings while maintaining full backward compatibility.

## Features

- **Backward Compatible**: All existing CLI arguments continue to work exactly as before
- **Precedence**: CLI arguments override config file values, maintaining expected behavior
- **Optional**: Configuration files are completely optional
- **Type Safe**: TOML deserialization provides type checking and validation
- **Extensible**: Easy to add new configuration options
- **Self-Documenting**: TOML files support comments for documentation

## Usage

### Using a Configuration File

1. **Generate a sample configuration file:**
   ```bash
   agave-validator generate-config --output my-config.toml
   ```

2. **Edit the configuration file** to match your needs (all fields are optional)

3. **Run the validator with the configuration file:**
   ```bash
   agave-validator --config my-config.toml
   ```

### CLI Arguments Override Configuration

CLI arguments always take precedence over configuration file values:

```bash
# This will use port 8900 instead of the value in config.toml
agave-validator --config config.toml --rpc-port 8900
```

### Hybrid Configuration

You can mix configuration files with CLI arguments:

```bash
# Use config file for most settings, but override specific ones via CLI
agave-validator --config config.toml \
    --entrypoint custom-entrypoint.com:8001 \
    --ledger /custom/ledger/path
```

## Configuration File Format

The configuration file uses TOML format with the following structure:

```toml
# Network Configuration
bind_address = "127.0.0.1"
entrypoint = [
    "entrypoint.mainnet-beta.solana.com:8001",
    "entrypoint2.mainnet-beta.solana.com:8001"
]
gossip_port = 8001
dynamic_port_range = "8002-10000"

# Ledger Configuration
ledger_path = "./ledger"
accounts_path = ["./accounts"]
limit_ledger_size = 50000000

# RPC Configuration
rpc_port = 8899
rpc_bind_address = "127.0.0.1"
enable_rpc_transaction_history = false
rpc_threads = 4

# Performance Configuration
accounts_shrink_ratio = 0.8
banking_trace_dir_byte_limit = 1000000000
tpu_connection_pool_size = 4

# Snapshot Configuration
full_snapshot_archive_interval_slots = 25000
incremental_snapshot_archive_interval_slots = 100
snapshot_version = "V1_7_0"

# Identity and Security
identity = "./validator-keypair.json"
vote_account = "./vote-account-keypair.json"
known_validators = []

# Validator Behavior
voting_disabled = false
dev_halt_at_slot = 0
wait_for_supermajority = 0
```

## Configuration Sections

### Network Configuration
- `bind_address`: IP address to bind validator ports
- `entrypoint`: List of gossip entrypoint addresses
- `gossip_port`: Port for gossip protocol
- `gossip_host`: DNS name or IP for gossip advertising
- `dynamic_port_range`: Range for dynamically assigned ports
- `allow_private_addr`: Allow private network addresses

### Ledger Configuration
- `ledger_path`: Directory for ledger storage
- `accounts_path`: List of directories for account storage
- `account_snapshot_paths`: Directories for account snapshots
- `limit_ledger_size`: Maximum ledger size in shreds

### RPC Configuration
- `rpc_port`: JSON RPC server port
- `rpc_bind_address`: IP address for RPC server
- `enable_rpc_transaction_history`: Enable transaction history storage
- `enable_extended_tx_metadata_storage`: Store extended transaction metadata
- `rpc_threads`: Number of RPC worker threads
- `rpc_blocking_threads`: Number of RPC blocking threads

### Performance Configuration
- `accounts_shrink_ratio`: Ratio for account storage shrinking
- `banking_trace_dir_byte_limit`: Banking trace directory size limit
- `tpu_connection_pool_size`: TPU connection pool size
- `tpu_max_connections_per_peer`: Max connections per peer
- `tpu_max_staked_connections`: Max staked connections
- `tpu_max_unstaked_connections`: Max unstaked connections

### Snapshot Configuration
- `snapshot_version`: Snapshot format version
- `snapshot_archive_format`: Compression format for snapshots
- `full_snapshot_archive_interval_slots`: Interval for full snapshots
- `incremental_snapshot_archive_interval_slots`: Interval for incremental snapshots
- `maximum_full_snapshot_archives_to_retain`: Number of full snapshots to keep
- `maximum_incremental_snapshot_archives_to_retain`: Number of incremental snapshots to keep

### Thread Configuration
- `replay_forks_threads`: Number of replay fork threads
- `replay_transactions_threads`: Number of replay transaction threads
- `tvu_shred_sigverify_threads`: Number of TVU shred signature verification threads

### Validator Behavior
- `voting_disabled`: Disable voting
- `dev_halt_at_slot`: Halt validator at specific slot (development)
- `wait_for_supermajority`: Wait for supermajority at slot
- `expected_genesis_hash`: Expected genesis hash
- `expected_bank_hash`: Expected bank hash
- `expected_shred_version`: Expected shred version
- `no_voting`: Disable voting (alternative flag)

### Identity and Security
- `identity`: Path to validator identity keypair
- `vote_account`: Path to vote account keypair
- `authorized_voter_keypairs`: List of authorized voter keypairs
- `known_validators`: List of known validator public keys
- `only_known_rpc`: Only allow RPC requests from known validators

### Development and Debugging
- `log_messages_bytes_limit`: Maximum bytes for log messages
- `skip_startup_ledger_verification`: Skip ledger verification at startup
- `debug_keys`: List of debug public keys

## Migration from CLI-only Configuration

1. **Generate config from current CLI usage:**
   ```bash
   # Your current command:
   agave-validator --ledger ./ledger --rpc-port 8899 --entrypoint entrypoint.com:8001
   
   # Generate config file with these settings:
   agave-validator generate-config \
       --ledger ./ledger \
       --rpc-port 8899 \
       --entrypoint entrypoint.com:8001 \
       --output my-config.toml
   ```

2. **Review and customize the generated configuration file**

3. **Test with the new configuration:**
   ```bash
   agave-validator --config my-config.toml
   ```

## Best Practices

1. **Use version control**: Store configuration files in your repository
2. **Environment-specific configs**: Use different configs for different environments
3. **Comment your settings**: Document why specific values were chosen
4. **Start simple**: Begin with basic settings and add more as needed
5. **Validate configs**: Always test configuration changes in a safe environment

## Troubleshooting

### Configuration File Not Found
```
Configuration error: Failed to read config file 'config.toml': No such file (os error 2)
```
**Solution**: Check the file path and ensure the file exists.

### Invalid TOML Syntax
```
Configuration error: Failed to parse TOML config: expected newline, found an identifier at line 5
```
**Solution**: Check TOML syntax. Common issues include missing quotes around strings or incorrect array syntax.

### Type Mismatch
```
Configuration error: Failed to parse TOML config: invalid type: string "abc", expected u16 for key `rpc_port`
```
**Solution**: Ensure numeric values are not quoted and match the expected type.

## Examples

### Mainnet Configuration
```toml
entrypoint = [
    "entrypoint.mainnet-beta.solana.com:8001",
    "entrypoint2.mainnet-beta.solana.com:8001",
    "entrypoint3.mainnet-beta.solana.com:8001"
]
ledger_path = "/opt/solana/ledger"
accounts_path = ["/opt/solana/accounts"]
rpc_port = 8899
enable_rpc_transaction_history = true
identity = "/opt/solana/validator-keypair.json"
vote_account = "/opt/solana/vote-account-keypair.json"
```

### Development Configuration
```toml
bind_address = "127.0.0.1"
ledger_path = "./dev-ledger"
rpc_port = 8899
allow_private_addr = true
dev_halt_at_slot = 1000
skip_startup_ledger_verification = true
```

### High-Performance Configuration
```toml
accounts_shrink_ratio = 0.9
banking_trace_dir_byte_limit = 5000000000
tpu_connection_pool_size = 8
tpu_max_staked_connections = 4000
rpc_threads = 8
rpc_blocking_threads = 4
``` 