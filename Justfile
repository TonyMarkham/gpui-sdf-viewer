set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"]

semantic_graph_extract := ".refactor-radar/bin/semantic-graph-extract"
rust_workspace := "rust-workspace"
csharp_solution := "csharp-solution"
soul_workspace := "soul-workspace"
fts := "fts"

confidence:
    clear
    @echo "// *===== cargo fmt ============================================================================* //"
    @echo ""
    cargo fmt
    @echo ""
    @echo "// *===== cargo check ==========================================================================* //"
    @echo ""
    cargo check
    @echo ""
    @echo "// *===== cargo clippy =========================================================================* //"
    @echo ""
    cargo clippy --all-targets -- -D warnings
    @echo ""
    @echo "// *===== cargo build ==========================================================================* //"
    @echo ""
    cargo build
    @echo ""
    @echo "// *===== cargo test ===========================================================================* //"
    @echo ""
    cargo test
    @echo ""
    @echo "// *===== cargo build --release ================================================================* //"
    @echo ""
    cargo build --release
    @echo ""

separator:
    @echo ""
    @echo "// *============================================================================================* //"
    @echo ""

refresh:
    @echo "// *===== .refactor-radar/bin/semantic-graph-extract rust-workspace ============================* //"
    @echo ""
    {{semantic_graph_extract}} {{rust_workspace}} --progress
    @echo ""
    @echo "// *===== .refactor-radar/bin/semantic-graph-extract fts =======================================* //"
    @echo ""
    {{semantic_graph_extract}} {{fts}} --progress
    @echo ""
    @echo "// *===== .refactor-radar/bin/semantic-graph-extract soul-workspace ============================* //"
    @echo ""
    {{semantic_graph_extract}} {{soul_workspace}} --progress
    @echo ""

soul:
    @echo "// *===== ./.soul/indexer index ================================================================* //"
    @echo ""
    ./.soul/indexer index
    @echo ""
    @echo "// *============================================================================================* //"
