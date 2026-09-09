use gpui_tea::Composite;

enum Msg {}

#[derive(Composite)]
#[composite(message = Msg)]
union Parent {
    field: usize,
}

fn main() {}
