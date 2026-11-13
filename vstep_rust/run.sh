#!/usr/bin/env bash
cd "$(dirname "$0")"
export PORT=${PORT:-5050}

# Cek jika port sedang dipakai proses lama, bersihkan otomatis
PID=$(lsof -ti :$PORT 2>/dev/null)
if [ -n "$PID" ]; then
    echo "Port $PORT sedang dipakai oleh proses (PID: $PID). Membersihkan proses lama..."
    kill -9 $PID 2>/dev/null
    sleep 0.5
fi

echo "============================================================"
echo "⚡ Menjalankan VolStep Finder (Rust Version) pada port $PORT..."
echo "============================================================"
if [ -f "./target/release/vstep_rust" ]; then
    ./target/release/vstep_rust
else
    source $HOME/.cargo/env 2>/dev/null
    cargo run --release
fi
