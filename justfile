setup:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f .env ]; then
        cp .env.example .env
        echo "✅ 已从 .env.example 创建 .env"
        echo "⚠️  请检查 .env 中的 DATABASE_URL / JWT_SECRET / S3_* 等配置"
    else
        echo "✅ .env 已存在"
    fi
    echo "🚀 启动数据库..."
    docker compose up -d 2>/dev/null || true
    echo "📦 运行迁移..."
    cargo run --package migrator 2>&1 | tail -3
    echo "--- Setup complete ---"

dev:
    #!/usr/bin/env bash
    set -euo pipefail
    export RUSTC_WRAPPER=sccache
    cargo watch -q -c -x "run --package server" -w bin/server -w features -w infrastructure -w libs

check:
    #!/usr/bin/env bash
    set -euo pipefail
    export RUSTC_WRAPPER=sccache
    cargo watch -q -c -x "check --workspace" -w bin/server -w features -w infrastructure -w libs

e2e:
    #!/usr/bin/env bash
    set -euo pipefail
    # 分文件顺序执行 + 间隔，避免 `axum_governor` 默认限流在同一次高密度连打时报 429（单进程 `hurl … e2e/` 会一次跑满多域用例）。
    files=(e2e/file.hurl e2e/identity.hurl e2e/erp_foundations.hurl e2e/p2_purchase_sales.hurl e2e/p3_production.hurl e2e/p4_finance_planning.hurl e2e/p5_cost_finance_mrp.hurl e2e/health.hurl)
    last=$((${#files[@]} - 1))
    for i in "${!files[@]}"; do
      hurl --test --variables-file e2e/env "${files[$i]}"
      if [ "$i" -lt "$last" ]; then
        sleep 2
      fi
    done

build:
    cargo build --package server --release --locked
    ls -lh target/release

build_release_ci:
    cargo build --package server --profile release-ci --locked
    ls -lh target/release-ci

build_linux_x86_64_gnu:
    cargo build --package server --release --target x86_64-unknown-linux-gnu --locked
    ls -lh target/x86_64-unknown-linux-gnu/release

build_linux_x86_64_gnu_ci:
    cargo build --package server --profile release-ci --target x86_64-unknown-linux-gnu --locked
    ls -lh target/x86_64-unknown-linux-gnu/release-ci

sqlx_up:
    cargo run --package migrator

who-uses symbol:
    @rg -l --type rust '{{symbol}}' features cross_domain bin libs infrastructure | sort

ai-check crate:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Architecture test ==="
    cargo test -p server arch_test --quiet 2>&1 | tail -3
    echo "=== Format check ===
    cargo fmt --all -- --check 2>&1 | tail -3
    echo "=== Check {{crate}} ==="
    cargo check -p {{crate}} --message-format=short 2>&1 | tail -5
    echo "=== Test {{crate}} ==="
    cargo test -p {{crate}} --quiet 2>&1 | tail -10
    echo "--- Done ---"

pre_commit:
    cargo machete
    cargo sort features/**
    cargo sort
    cargo fmt --all
    cargo clippy --workspace
