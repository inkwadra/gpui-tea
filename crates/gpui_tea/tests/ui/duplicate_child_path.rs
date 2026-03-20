use gpui_tea::Composite;

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
#[composite(message = Msg)]
struct Parent {
    #[child(
        path = "first",
        path = "second",
        lift = Msg::Child,
        extract = Msg::into_child
    )]
    child: Child,
}

fn main() {}
