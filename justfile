default: run-dev

clean:
  cargo clean

check:
  cargo check

clippy:
  cargo clippy -- -W clippy::pedantic

clippy-fix:
  cargo clippy --fix -- -W clippy::pedantic

fmt:
  cargo fmt

build-dev:
  cargo build

build-release:
  cargo build --release

# Build and install the extension status plugin into a local WPBS checkout.
install-google-ping wpbs_dir="../wpbs": build-release
  mkdir -p "{{wpbs_dir}}/plugins/binaries/local/google-ping/0.1.0"
  cp target/wasm32-wasip2/release/google_ping.wasm "{{wpbs_dir}}/plugins/binaries/local/google-ping/0.1.0/plugin.wasm"
  cp google-ping/metadata.json "{{wpbs_dir}}/plugins/binaries/local/google-ping/0.1.0/metadata.json"

run-dev:
  cargo run

run-release:
  cargo run --release
