//! Generates typed code for the `*.layout.xml` files under `src/` (see `bg_layout`).

fn main() {
    bg_layout::codegen::build("src");
}
