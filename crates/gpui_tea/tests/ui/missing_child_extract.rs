use gpui_tea::Composite;

enum Msg {
    Child(()),
}

#[derive(Composite)]
#[composite(message = Msg)]
struct Parent {
    #[child(path = "child", lift = Msg::Child)]
    child: Child,
}

struct Child;

fn main() {}
