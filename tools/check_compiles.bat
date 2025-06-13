echo off
rem Pick the ones your script tests
rem (either keep everything on one line, or use the CMD line-continuation ^)

rem ----- one-line version (simplest) -----
rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-gnu x86_64-apple-darwin

rem ----- or split across lines with ^ -----
rem rustup target add ^
rem    x86_64-unknown-linux-gnu ^
rem    x86_64-pc-windows-gnu ^
rem    x86_64-apple-darwin

cargo check --target x86_64-unknown-linux-gnu
cargo check --target x86_64-pc-windows-gnu
cargo check --target x86_64-apple-darwin

pause