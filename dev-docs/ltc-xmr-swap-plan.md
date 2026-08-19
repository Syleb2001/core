# Plan: LTC→XMR atomic swaps (ASB-first)

Working document for adding a Litecoin→Monero swap pair
alongside the existing Bitcoin→Monero pair.
Direction: the taker (Bob) locks LTC and receives XMR;
the ASB (Alice) sells XMR and receives LTC —
the exact mirror of today's BTC→XMR flow,
which is the natural direction of the protocol
(the "script chain" is always locked by Bob,
Monero is always locked by Alice).

Scope: ASB / backend first.
No GUI work in this phase,
but every Rust crate (including `src-tauri`) must keep compiling.
Per `AI_POLICY.md`, changes are piloted by a human;
this document is the map, not an autopilot.

## 1. Why this is feasible

The cryptographic core of the protocol is chain-agnostic.
Everything the swap needs from the script chain exists identically on Litecoin:

- P2WSH (segwit v0, active on LTC since 2017) and the shared-output miniscript
  `c:and_v(v:pk(A),pk_k(B))` (`swap-core/src/bitcoin.rs:222`).
- Relative timelocks via BIP68 `nSequence` (TxCancel, TxPunish, TxReclaim).
- ECDSA on secp256k1 with `SIGHASH_ALL`,
  including the adaptor-signature (encsig) construction from `ecdsa_fun`.
- Transaction, script, sighash and PSBT (BIP174) serialization are byte-identical;
  PSBTs carry scripts, not addresses, so `Message2.tx_lock_psbt` is chain-neutral.
- 8 decimal places (1 LTC = 10^8 litoshi),
  so `bitcoin::Amount` remains numerically valid.

What actually differs:

| Aspect | Bitcoin | Litecoin |
| --- | --- | --- |
| bech32 HRP | `bc` / `tb` / `bcrt` | `ltc` / `tltc` / `rltc` |
| base58 prefixes | `1`/`3`, `m,n`/`2` | `L`/`M`, `m,n`/`Q` (unused: bech32-only policy) |
| SLIP-44 coin type | 0 (testnet 1) | 2 (testnet 1) |
| Avg block time | 10 min | 2.5 min (all timelocks ×4 for wall-clock parity) |
| Fee API | mempool.space | litecoinspace.org (same mempool.space codebase/API — to verify) |
| Electrum servers | curated list exists | new list to curate (ElectrumX-LTC / Fulcrum ecosystem) |
| Hashrate/finality | finality_confirmations = 1 | lower hashrate; propose 2 (decision) |
| MWEB | n/a | exists, deliberately unsupported (bech32 v0 only) |

## 2. Bitcoin-specific inventory (from codebase analysis)

The workspace is already well modularized;
the port cost is concentrated in known places:

- **`swap-env/src/env.rs:8-28`** —
  9 `bitcoin_*` fields (network, avg block time, 3 timelocks, finality, timeouts)
  selected per `Mainnet/Testnet/Regtest`.
- **`swap-env/src/config.rs`** —
  singular `[bitcoin]` section (`:58-71`),
  `min_buy_btc`/`max_buy_btc` (`:147-150`),
  `external_bitcoin_redeem_address` (`:186`),
  defaults and prompts (`defaults.rs:60-101`, `prompt.rs`).
- **`bitcoin-wallet`** —
  BIP84 templates (coin type 0/1) at `wallet.rs:531,642`;
  mempool.space hardcoded (`wallet.rs:2659,2686-2692`);
  address validation restricted to P2WPKH with Bitcoin HRPs (`core.rs:68-146`);
  `is_testnet: bool` shortcuts (4 sites in `core.rs`);
  dust/fee constants (`wallet.rs:83-88`);
  wallet dir names not chain-scoped (`wallet.rs:374-376`).
- **`swap-p2p`** — 8 protocol ids under `/comit/xmr/btc/...`
  (quote, swap_setup, transfer_proof, encrypted_signature,
  cooperative_xmr_redeem_after_punish, wormhole, identify ×2),
  rendezvous namespaces `xmr-btc-swap-{mainnet,testnet}` (`rendezvous.rs:11-13`),
  `BlockchainNetwork { bitcoin, monero }` (`swap_setup.rs:36-42`).
- **`swap-feed`** — pair strings hardcoded per provider:
  Kraken `XMR/XBT`, Bitfinex `tXMRBTC`, KuCoin `XMR-BTC`, Exolix `BTC→XMR`.
  Aggregation (`ExchangeRate`, validity, 10% inter-exchange guard) is reusable as-is.
- **`swap-db` / `swap/migrations`** —
  no chain column anywhere;
  states serialized as JSON with `bitcoin::Address` strings;
  serde backcompat is fragile
  (`RefundSignatures` is `#[serde(untagged)]` + flattened — never rename).
- **`swap/src/asb/event_loop.rs`** —
  `capture_wallet_snapshot` (9 fee estimates), `make_quote`, anti-spam policy:
  all `bitcoin::Amount`-typed but chain-neutral in logic.
- Already-good seams:
  `EventLoop` holds `Arc<dyn BitcoinWallet>` (trait at `bitcoin-wallet/src/lib.rs:15`),
  `swap-machine` is pure,
  `monero-wallet` has zero bitcoin coupling
  (only `swap-core/src/monero/{primitives,ext}.rs` need touching).

## 3. Strategy

**S1 — Runtime `Chain` parameter, not type-level generics.**
Introduce `Chain { Bitcoin, Litecoin }` plus a `ChainParams` descriptor
(HRPs, SLIP-44 coin type, avg block time, timelock defaults,
dust/min-relay constants, fee API base URL, electrum defaults, explorer URL).
Keep all rust-bitcoin consensus types
(`Transaction`, `ScriptBuf`, `Psbt`, `Amount`, `Txid`) —
they are byte-compatible with Litecoin.
Internally the BDK wallet runs on the corresponding
`bitcoin::Network::{Bitcoin,Testnet,Regtest}` ("shadow network").

**S2 — Chain-aware address type, discriminated by HRP.**
A `ChainAddress` (wrapping witness program + chain + network)
that parses `bc1…`/`ltc1…` by HRP and displays the correct form.
Backward compatible with existing DB blobs and wire strings
(`bc1…` parses as Bitcoin).
Only bech32 v0 accepted (P2WPKH user addresses, P2WSH internal),
matching the existing policy;
MWEB addresses (`ltcmweb1…`) are naturally rejected.

**S3 — One pair per ASB process (phase 1).**
The config selects the script chain
(`[bitcoin]` or `[litecoin]` section, exactly one).
EventLoop, swap runners and DB stay single-pair per process;
running BTC and LTC side by side = two processes with separate data dirs.
Multi-pair in one process is a later milestone
(needs shared XMR reserve accounting).

**S4 — New protocol id family, full backward compatibility.**
`/comit/xmr/ltc/{swap_setup,bid-quote,transfer_proof,encrypted_signature,cooperative_xmr_redeem_after_punish}/…`
and rendezvous namespaces `xmr-ltc-swap-{mainnet,testnet}`.
BTC peers never negotiate with LTC endpoints;
old clients fail cleanly at protocol negotiation.
Wormhole stays shared (transport-level, chain-agnostic).

**S5 — XMR/LTC rate via cross-rate.**
No liquid direct XMR/LTC pair on the big exchanges;
compose XMR/LTC = (XMR/BTC ask) ÷ (LTC/BTC bid) per provider
(Kraken, Bitfinex, KuCoin subscribe both legs; Exolix quotes LTC→XMR directly).
Both legs must be fresh for the sample to count;
reuse the existing validity/spread guards.

**S6 — Timelocks at wall-clock parity (×4).**

| Constant | BTC mainnet | LTC mainnet (proposed) | Wall clock |
| --- | --- | --- | --- |
| cancel_timelock | 24 | 96 | ~4 h |
| punish_timelock | 144 | 576 | ~24 h |
| remaining_refund_timelock | 2 | 8 | ~20 min |
| finality_confirmations | 1 | 2 (decision) | ~5 min |
| avg_block_time | 10 min | 2.5 min | — |
| lock_confirmed_timeout | 2 h | 1 h | ~24 blocks |

## 4. Milestones and tasks

### M0 — Preparation (pure refactors, zero behavior change)

- [x] Introduce `Chain` + `ChainParams` in a new bottom-of-graph crate
      (`swap-chain`: identity, tickers, HRPs, SLIP-44 coin type,
      fee/explorer endpoints — consensus timing stays in `swap-env`).
- [x] Tag `env::Config` with its script chain (`chain: Chain` field
      + `chain_params()` accessor).
      Chosen over mass-renaming the `bitcoin_*` fields:
      this fork tracks upstream,
      and a rename would conflict with every upstream change
      touching those ~100 use sites.
      The fields are documented as "script chain" values instead.
- [ ] *(moved to M1)* Replace the `is_testnet: bool` address shortcuts
      (`bitcoin-wallet/src/core.rs`) together with the `ChainAddress` codec.
- [ ] *(moved to M4)* Neutralize `AmountExt::max_bitcoin_for_price`
      (`swap-core/src/monero/primitives.rs:100-147`)
      when the rate plumbing is touched anyway.
- [ ] Gate: `cargo c --all-features --all-targets`, unit tests green,
      one BTC docker happy-path run to prove zero regression
      (docker is unavailable in this dev environment —
      the happy-path run is on the pilot's machine).

### M1 — Litecoin chain layer

- [x] `ChainParams` for Litecoin: HRPs, coin type 2,
      fee API base URL, explorer URL (in `swap-chain`;
      dust/min-relay verification against litecoind defaults moved
      to the M5 harness work where litecoind is actually running).
- [x] `ChainAddress` codec (parse/display by HRP, bech32 v0 only)
      with string serde, shadow-address mapping for bdk interop,
      round-trip and rejection tests (wrong chain/network, taproot,
      unknown HRP, P2WSH-vs-P2WPKH guard).
- [x] `bitcoin-wallet`: `chain` on `WalletConfig` (default Bitcoin) and
      `Wallet`; per-chain descriptors — Bitcoin keeps bdk's `Bip84`
      template byte-for-byte (locked by test), Litecoin derives at
      `m/84'/2'/0'` on mainnet and `m/84'/1'/0'` on testnets;
      chain-scoped wallet directory (`wallet-litecoin`);
      legacy pre-BDK-1.0 migration skipped for non-Bitcoin chains;
      mempool.space client resolves its base URL from `ChainParams`
      (litecoinspace.org for LTC mainnet/testnet, unavailable on regtest).
- [ ] **Early spike (de-risk):** create an LTC regtest wallet against
      litecoind + an LTC electrum server;
      verify BDK sync/broadcast with the shadow network
      (genesis-hash anchor behavior), fund, send, `max_giveable`.
- [ ] Unit tests for the codec and params.

### M2 — Env and ASB config

- [x] `env::Config` constructors for LTC
      (`LitecoinMainnet`/`LitecoinTestnet`/`LitecoinRegtest`;
      timelock table above, locked by a wall-clock-parity test).
- [x] `[litecoin]` config section — `Config.bitcoin`/`.litecoin` are now
      options with exactly-one enforced by `Config::script_chain()`;
      `env::new` selects the environment from the configured chain
      (this also revived the config file's `finality_confirmations`
      override, which previously had no caller).
      `min_buy`/`max_buy` stay in the `min_buy_btc`/`max_buy_btc` keys
      for now (denominating the script chain); renaming is an M4 cleanup.
- [x] Default LTC electrum server candidates
      (`electrum-ltc.bysh.me`, `electrum.ltc.xurious.com`,
      `backup.electrum-ltc.org`; `ltc.rentonrisk.com` dropped — dead DNS).
      `dev-scripts/health_check_default_electrum_servers.py` had two
      stale paths and is fixed; the dev sandbox blocks raw TCP, so run
      the script from an unrestricted machine to verify the candidates.
- [x] Prompts/defaults for the interactive setup: chain question first,
      ticker-aware prompts, LTC buy-amount defaults,
      data/config dirs `mainnet-ltc`/`testnet-ltc`, listen ports 9739/9639.
- [x] ASB startup (`swap-asb/src/main.rs`): env recomputed from the config
      file (the chain lives there, not in CLI flags), wallet built with
      the configured chain.
- [ ] *(moved to M4)* Chain-aware address validation for the `WithdrawBtc`
      CLI/RPC path — until then, withdrawing from a Litecoin ASB is
      unsupported (BTC-shaped address validation would reject `ltc1…`).

### M3 — P2P layer

- [x] Protocol ids resolved per chain: the `/comit/xmr/ltc/...` family for
      bid-quote, swap_setup, transfer_proof, encrypted_signature and
      cooperative_xmr_redeem_after_punish; Bitcoin id strings untouched
      and locked by tests (they are a network-wide convention).
- [x] Identify protocol version per chain
      (`swap_p2p::protocols::identify_protocol_version`).
- [x] `BlockchainNetwork` unchanged: the new protocol ids discriminate the
      chain, the `bitcoin` field carries the shadow network on both sides.
- [x] The ASB registers under the chain-scoped rendezvous namespace
      (`xmr-ltc-swap-{mainnet,testnet}`); the taker/CLI side keeps the
      Bitcoin namespaces until the taker milestone (M6/GUI).
- [x] Negotiation test: a Litecoin taker talking to a Bitcoin maker fails
      cleanly with DoesNotSupportProtocol (`swap-p2p` quotes test).

### M4 — ASB event loop, feed, DB

- [x] `swap-feed`: Kraken serves the XMR/LTC cross rate
      (dual XMR/XBT + LTC/XBT subscription; ask ÷ bid, integer litoshi
      math, emitted only while both legs are < 10 min old) and Exolix
      quotes LTC→XMR directly. Bitfinex and KuCoin refuse to start for
      a Litecoin ASB with an explicit config hint — adding their cross
      legs is follow-up work, not silent mispricing. `FixedRate`
      unchanged (already chain-neutral).
- [x] DB migration: `swap_states.chain` (default `'bitcoin'`);
      the database opens for one chain — inserts tag it, listings and
      the resume path only see it, loading a foreign-chain swap fails
      with an explicit error.
- [ ] Event loop: quotes, wallet snapshot (9 fee estimates), anti-spam policy —
      logic unchanged, amounts now denominate LTC;
      min/max enforcement in `swap_setup/alice.rs` unchanged (verify).
- [ ] Swap runner (`protocol::alice::run` / `bob::run`):
      should need no logic change — verify timelock/fee paths use env config only.
- [ ] Controller/RPC + tauri layer: chain-aware address validation for
      `WithdrawBtc`/`set_external_bitcoin_redeem_address`
      (until then a Litecoin ASB cannot withdraw via CLI/RPC);
      neutralize `AmountExt::max_bitcoin_for_price` and the
      "XMR/BTC" display labels in `swap-asb`.

### M5 — Integration tests (docker)

- [ ] Research + pin images: litecoind (e.g. `uphold/litecoin-core`)
      and an LTC-capable electrum server
      (Fulcrum recommended; `vulpemventures/electrs` is BTC-only).
- [ ] Harness: parameterize `swap/tests/harness` by chain
      (containers, HRP, fallbackfee flag, generate-blocks).
- [ ] Tests: `happy_path_ltc`, refund path, punish path, early-refund path
      (start with these four; extend to the amnesty family after).
- [ ] `justfile` targets + CI matrix entries.
- [ ] Gate (AI_POLICY): full LTC docker suite green + BTC suite unchanged.

### M6 — Ops and finish

- [ ] `swap-orchestrator`: litecoind/electrum container definitions + compose.
- [ ] Docs: ASB README section, changelog entry.
- [ ] Real-world dry run on LTC testnet (testnet4) end to end.

### M7 — Later (out of scope for now)

- Multi-pair in a single ASB process (shared XMR reserve accounting).
- GUI (address display, explorer links `litecoinspace.org/tx/…`, pair selector).
- Upstream discussion (protocol ids are a network-wide convention).

## 5. Open decisions (recommendation first)

1. **Process model:** one pair per ASB process (recommended) vs multi-pair.
2. **Address handling:** HRP-discriminated `ChainAddress` (recommended)
   vs internal shadow encoding re-encoded at edges.
3. **LTC finality confirmations:** 2 (recommended, lower hashrate) vs 1 (BTC parity).
4. **Rate source:** cross-rate via BTC legs (recommended, liquid)
   vs direct XMR/LTC pairs only (thin markets).
5. **Timelock values:** validate the ×4 wall-clock-parity table above.
6. **Amounts:** reuse `bitcoin::Amount` for litoshi (recommended; display-level naming only).

## 6. Risks

- **BDK shadow-network genesis anchor** —
  local chain starts from the BTC genesis hash while the electrum server serves LTC;
  believed safe (anchors attach at tx heights), must be proven by the M1 spike first.
- **Serde backcompat of persisted states** —
  enum variant names are DB tags; `RefundSignatures` is untagged+flattened;
  no renames of serialized shapes, ever.
- **LTC electrum server ecosystem** —
  fewer/less reliable public servers; quorum (`min_parallel_responses: 2`)
  may need per-chain tuning.
- **CI images** — litecoind/Fulcrum images must be pinned by digest;
  regtest segwit activation must be verified.
- **Cross-rate correctness** — use the bid leg for LTC/BTC
  (maker-favorable), and require both legs fresh.

## 7. Validation checklist (every milestone)

- `cargo c --all-features`, `cargo c --tests`, `cargo c --all-targets`.
- Unit tests of touched crates; `cargo fmt` + clippy clean.
- Affected docker tests (liberal interpretation), BTC suite as regression guard.
- Atomic commits, changelog entry for user-visible changes (CONTRIBUTING.md).
