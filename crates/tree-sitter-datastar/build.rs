fn main() {
    cc::Build::new()
        .include("vendor/src")
        .file("vendor/src/parser.c")
        .file("vendor/src/scanner.c")
        .warnings(false)
        .compile("tree-sitter-datastar");
}
