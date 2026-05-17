# Test TLS Material

**This directory contains a self-signed certificate and its private key used
exclusively by the unit tests in `src/server/tls_test.rs`.**

The key is intentionally checked into the repository — it is a test fixture,
not a secret. Do not reuse this material outside of test code.

## Properties

- Subject: `CN=localhost`
- SAN: `DNS:localhost,IP:127.0.0.1`
- Validity: 100 years from generation
- Key: 2048-bit RSA, unencrypted

## Regenerating

```bash
openssl req -x509 -newkey rsa:2048 \
  -keyout crates/starknet_transaction_prover/resources/test_tls/key.pem \
  -out   crates/starknet_transaction_prover/resources/test_tls/cert.pem \
  -sha256 -days 36500 -nodes \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
```

## `key_mismatched.pem`

A second, unrelated private key used only to exercise the "certificate and
key don't match" failure path of `load_tls_acceptor`. It is a throwaway key
with no corresponding certificate — it is never paired with `cert.pem` for
anything other than asserting that pairing fails.

Regenerate with the same recipe, discarding the self-signed certificate it
also produces:

```bash
openssl req -x509 -newkey rsa:2048 \
  -keyout crates/starknet_transaction_prover/resources/test_tls/key_mismatched.pem \
  -out /dev/null \
  -sha256 -days 36500 -nodes \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
```
