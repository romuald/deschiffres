# WASM build directory
WASM_DIR := html/wasm/

default: debug

debug:
	cargo build

release:
	cargo build -r

test:
	cargo test

clean:
	cargo clean
	rm -f temp-wasm/* ${WASM_DIR}}/*
	rmdir -f temp-wasm

wasm:
	wasm-pack build --target web -d temp-wasm/ --no-typescript --release --features wasm
	cp temp-wasm/*.js temp-wasm/*.wasm ${WASM_DIR}
