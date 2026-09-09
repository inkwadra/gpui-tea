use gpui_tea::Composite;

#[derive(Clone)]
enum Msg {
    Child(()),
}

impl Msg {
    fn into_child(self) -> Result<(), Self> {
        match self {
            Self::Child(msg) => Ok(msg),
        }
    }
}

struct Child;

#[derive(Composite)]
#[child(path = "child", lift = Msg::Child, extract = Msg::into_child)]
#[composite(message = Msg)]
struct Parent {
    child: Child,
}

fn main() {}
