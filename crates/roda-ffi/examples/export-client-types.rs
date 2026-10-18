#[cfg(feature = "client-types")]
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: export-client-types OUTPUT_DIRECTORY");
    roda_ffi::export_typescript(std::path::Path::new(&path)).expect("export client types");
}

#[cfg(not(feature = "client-types"))]
fn main() {
    panic!("Enable --features client-types to export the application contract.");
}
