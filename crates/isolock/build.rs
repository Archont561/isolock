fn main() {
    // clifacet 0.10's controller macros emit these optional-feature cfgs in the
    // consuming crate. Register their names for rustc's cfg checker only: this
    // enables no Cargo features and leaves warnings for unknown cfgs enabled.
    println!(
        "cargo::rustc-check-cfg=cfg(feature, values(\"async\", \"completions\", \
         \"mcp\", \"schema\", \"skill\", \"skill-minijinja\"))"
    );
}
