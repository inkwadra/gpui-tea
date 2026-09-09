use gpui_tea::Composite;

enum Msg {}

#[derive(Composite)]
#[composite(message = Msg)]
struct Parent(usize);

fn main() {}
