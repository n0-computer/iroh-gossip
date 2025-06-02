# sim-frontend

Runner and renderer for iroh-gossip simulations

The frontend uses vite and react.
The simulator is compiled to wasm and can run right in the browser.
You can either load simulations created with the `sim` binary, or run simulations in the browser.

Usage:

```
npm run build:wasm
npm install
npm run dev
```

Precreate simulations to load:
```
cargo run --bin sim --features simulator-bin --release -- run -c ../simulations/all.toml -o data
```
