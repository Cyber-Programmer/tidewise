# netdeps fixture workspace

Fake workspace for `crates/core/tests/no_network_deps.rs`. `reqwest` is an empty local crate that only
carries the name. `leaky` reaches it through `middle` (a transitive dependency); `clean` has none.
The test copies this folder to a temp dir before running `cargo tree --offline`, so no `Cargo.lock` or
`target/` is written here. Nothing here is built by the real workspace.
