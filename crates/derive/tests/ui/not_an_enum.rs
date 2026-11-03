use nvui_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
struct Event {
	method: String,
}

fn main() {}
