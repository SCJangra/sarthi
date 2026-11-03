use nvui_derive::DeserializeTaggedEnum;

struct NotDeserialize;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Grid(#[tagged_enum(flatten)] Vec<NotDeserialize>),
	Other { method: String, value: rmpv::Value },
}

fn main() {}
