fn main() {
    // SQLx embeds migrations at compile time. Also detect newly added SQL files.
    println!("cargo:rerun-if-changed=migrations");
}
