# Pick the ones your script tests
rustup target add \
  x86_64-unknown-linux-gnu \
  x86_64-pc-windows-gnu \
  x86_64-apple-darwin

cargo check --target x86_64-unknown-linux-gnu    # Linux build
cargo check --target x86_64-pc-windows-gnu       # Windows cross-build
cargo check --target x86_64-apple-darwin         # macOS cross-build