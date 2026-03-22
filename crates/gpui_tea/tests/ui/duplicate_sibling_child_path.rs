use gpui_tea::Composite;

enum Msg {
    First(()),
    Second(()),
}

impl Msg {
    fn into_first(self) -> Result<(), Self> {
        match self {
            Self::First(msg) => Ok(msg),
            other => Err(other),
        }
    }

    fn into_second(self) -> Result<(), Self> {
        match self {
            Self::Second(msg) => Ok(msg),
            other => Err(other),
        }
    }
}

struct FirstChild;
struct SecondChild;

#[derive(Composite)]
#[composite(message = Msg)]
struct Parent {
    #[child(path = "shared", lift = Msg::First, extract = Msg::into_first)]
    first: FirstChild,
    #[child(path = "shared", lift = Msg::Second, extract = Msg::into_second)]
    second: SecondChild,
}

fn main() {}
