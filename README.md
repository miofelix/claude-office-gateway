# Claude Office Gateway

Cross-platform macOS and Windows client for connecting compatible Claude Office add-ins to a model provider through a private localhost HTTPS gateway.

The client uses a Tauri 2 shell with a Rust gateway core. The macOS implementation is being migrated from the original native Swift app, while Windows shares the same configuration, protocol adapter, and UI workflow.

## Development

```bash
pnpm install
pnpm tauri dev
```

The macOS build can be checked with:

```bash
pnpm typecheck
pnpm build:renderer
cargo test --manifest-path src-tauri/Cargo.toml
```

