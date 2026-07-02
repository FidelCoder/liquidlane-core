# LiquidLane Core

Backend service for vault accounting, liquidity requests, Fiber channel deployment, and fee distribution on LiquidLane.

## Development

```bash
cargo run
```

The API listens on `0.0.0.0:8080` by default.

```bash
curl http://localhost:8080/health
```

### Environment

- `LIQUIDLANE_BIND_ADDR`: server bind address, defaults to `0.0.0.0:8080`
- `LIQUIDLANE_ENV`: runtime environment label, defaults to `development`
