fn main() {
    println!("cargo:rerun-if-changed=../../app/infrastructure/migration/versions");
}
