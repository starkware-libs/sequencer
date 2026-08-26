# Measuring a version bump's effect on fees, with echonet

Replays one range of mainnet blocks twice — once at the version mainnet is on, once at the
version being rolled out — and reports what each transaction paid under both. Written for
0.14.3 vs 0.14.4; nothing in it is specific to those versions.

The sequencer's versioned constants are pinned process-wide at startup: the gateway fetches
`echonet/get_starknet_version` from the recorder and calls `set_effective_latest_version`. So
serving a different version there is enough to price a whole replay at a version mainnet never
ran. Everything else here exists to survive the consequences of doing that.

## The four switches

All read from the echonet pod's environment (`echonet/k8s/echonet/deployment.yaml`), all
neutral by default, so a deployment that sets none of them behaves exactly as it always has.

| Variable | Set it to | Why |
|---|---|---|
| `ECHONET_STARKNET_VERSION_OVERRIDE` | `""`, then the new version | What the sequencer prices with. Empty reports the replayed block's own version. |
| `ECHONET_FEE_CSV_RUN_LABEL` | a distinct label per run | Turns on fee recording and names the file, `/data/echonet/fees_<label>.csv`. |
| `ECHONET_RESYNC_ENABLED` | `false` | Divergence from mainnet is the point of the second pass. Resyncing on it would rewind and re-execute blocks already recorded. |
| `ECHONET_OS_RUNNER_ENABLED` | `false` | The OS program is pinned to the version mainnet runs, so every OS run in the second pass would only fail. |

## Running it

1. Set the baseline run's env (`""`, a label, resync and OS runner off) and
   `python3 echonet/deploy_echonet.py -n <ns> --block-hash-cli-binary <path>`.
2. When it has covered the range, change the override to the new version and the label to a
   new one, and deploy again. Echonet rewinds to `start_block` on every startup, so the second
   pass covers the same blocks without any further setup.
3. Pull each CSV: `kubectl exec <echonet-pod> -c echonet -- cat /data/echonet/fees_<label>.csv`,
   or `GET /echonet/fee_csv` for the run in progress.
4. `python -m echonet.fee_comparison_report --baseline A.csv --candidate B.csv
   --categories categories.json --out joined.csv`

`categories.json` maps a name to the addresses that define it, matched against both the sending
account and the contracts the calldata calls into — an operator that batches for a venue shows
up as a sender, an app shows up as a target, and either identifies it:

```json
{"extended": ["0x048ddc53f4…", "0x62da0780fa…"], "games": ["0x46da895582…"]}
```

There is no end block, so nothing stops on its own; watch the last row's block number and move
on when it passes your target.

## Three things that will bite you

**Check the CSV's `starknet_version` column before trusting a run.** If echonet is unreachable
when the gateway starts, it logs a warning and falls back to the compile-time `LATEST` instead
of failing — which will quietly run your "new version" pass at whatever version the binary was
built with. The column is the proof that the override took.

**A redeploy mid-experiment silently reverts to the neutral defaults**, because those are what
is committed. Anything that forces a redeploy — a scaled-down pod, a crash — needs the env set
again first, or you get a baseline run with no label.

**Two classes of transaction cannot survive the second pass**, both because their validity is
bound to a mainnet block hash that a diverged chain cannot reproduce:

- *Staking attestations* (`attest`) revert. In the 0.14.3→0.14.4 run this was every attestation
  in the range and nothing else — 96 of 3332 transactions.
- *Proof-carrying transactions* are rejected by the gateway forever
  (`Invalid proof facts: Block hash mismatch`). Worse, `transaction_sender` treats a
  deterministic `VALIDATE_FAILURE` like a transient one and burns all 200 retries holding the
  forwarding lock, stalling the whole run for minutes per occurrence.

Neither is a pricing effect. The report counts both separately rather than averaging them in:
`rev. only` for the outcome flips, `only in baseline` for the ones that never landed.

## Reading the numbers

`l2_gas = vm_cost + sierra_gas` (`fee/resources.rs`). Contract execution is `sierra_gas` and a
versioned-constants bump does not touch it; only `vm_cost`, the OS's own overhead, moves.

That overhead is a **max over resource dimensions, not a sum** (`fee_utils.rs`), and it is
rounded twice — to integer L1 gas at 25/10000 per step, then to sierra gas at 40000:1. The
result is a quantum of **40000 sierra gas per 400 VM steps**. A change smaller than one bucket
costs nothing at all, which is why a large share of transactions come out at exactly 0.000%.

So report the per-transaction distribution, not just the fee-weighted total. In the
0.14.3→0.14.4 run the total was +0.003% while 43% of transactions were unchanged, 42% rose by
up to +1.4%, and 15% fell — and one payer at 80% of all fees held the total flat single-handed.
The total answers "what does the network collect"; the distribution answers "what does a user
pay", and here they said opposite things.

## What the 0.14.3 → 0.14.4 run found

1000 blocks (11921400–11922416), 3332 baseline transactions reproducing mainnet fees exactly,
3170 comparable after exclusions.

| | Extended | Games | Other |
|---|---|---|---|
| transactions | 231 | 17 | 2922 |
| total fee change | −0.035% | +0.172% | +0.152% |
| median per transaction | −0.033% | +0.362% | +0.000% |

0.14.4 raised the flat per-transaction OS overhead (`InvokeFunction` 4348→4779 steps) and cut
most syscall costs. Large batched transactions amortise the constant to nothing and collect the
syscall discount many times over; mid-sized ones pay the constant without enough execution to
dilute it. Nothing was repriced — `step_gas_cost`, `builtin_gas_costs` and
`vm_resource_fee_cost` are identical between the two versions.

**This measures versioned constants only.** Gas prices are pinned to what mainnet recorded for
each block, so a fee-market or minimum-price change in the rollout's node config —
`min_l2_gas_price_per_height`, `snip35_target_atto_usd_per_l2_gas`, the L1 multipliers — is
invisible here and would dominate these numbers. Diff the prod config separately.
