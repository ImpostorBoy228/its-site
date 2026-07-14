all: build

build:
	go build -o itsite
	cd astroshit && cargo build --target wasm32-unknown-unknown && \
		mkdir -p ../static/wasm && \
		wasm-bindgen --target web --out-dir ../static/wasm --out-name astroshit --no-typescript target/wasm32-unknown-unknown/debug/astroshit.wasm

run: build
	./itsite

clean:
	rm -f itsite
	rm -rf static/wasm

.PHONY: all build run clean
