//! Warns when the built UI is missing, so a binary without one is never a surprise.

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let dist = std::path::Path::new(&manifest).join("../../web/dist");
    // Rebuild when the UI is rebuilt, so the embedded copy is never stale.
    println!("cargo:rerun-if-changed={}", dist.display());
    if !dist.join("index.html").is_file() {
        println!("cargo:warning=web/dist is missing: this build embeds no UI, and `grimoire serve` will say so. Build it with `npm run build` in web/, or build a release with scripts/release.");
    }
}
