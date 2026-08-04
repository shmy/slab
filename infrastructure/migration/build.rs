fn main() {
    println!("cargo:rerun-if-changed=sqlx.toml");
    println!("cargo:rerun-if-changed=versions");
}
