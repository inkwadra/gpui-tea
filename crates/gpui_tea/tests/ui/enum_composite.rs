use gpui_tea::Composite;

enum Msg {}

#[derive(Composite)]
#[composite(message = Msg)]
enum Parent {
    Variant,
}

fn main() {}
