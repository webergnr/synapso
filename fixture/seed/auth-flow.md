# Auth Flow

Every request from the desktop client passes through the gateway before it touches a service. Tokens are minted once and cached locally — the storage layer is covered in [adapter-design.md](adapter-design.md).

## Token exchange

```rust
fn exchange(code: &str) -> Result<Token> {
    let req = TokenRequest::new(code)
        .with_pkce(verifier);
    gateway.post("/oauth/token", req) // single mint
}
```

Session state lives in `TokenStore` (sled tree `auth/v2`) and is rebuilt on launch.

## Open items

- [x] Refresh rotation on 401
- [ ] Offline grace period — 72h?
- [ ] Revoke on device unlink

> The gateway is the only component allowed to see the client secret. Everything else gets short-lived JWTs.
