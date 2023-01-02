default: debug

debug:
	cargo build

release:
	cargo build -r

test:
	cargo test

wasm:
	wasm-pack build --target web --no-typescript --release --features wasm